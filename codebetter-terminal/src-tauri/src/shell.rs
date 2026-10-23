//! A tiny Unix-like shell over a sandboxed directory.
//!
//! The learner sees a virtual tree (`/home/<user>/Tasks.cb/...`) that is backed
//! by real files below `root`, so their own editor can open the files too.
//! Paths are normalised lexically and can never leave `root`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::i18n::{Lang, Settings};
use crate::runner;
use crate::tasks::{self, Progress, Task};
use crate::tr;

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const CYAN: &str = "\x1b[36m";

const TASKS_DIR: &str = "Tasks.cb";
const TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// (name, English help, German help)
const COMMANDS: &[(&str, &str, &str)] = &[
    ("help", "show this help", "diese Hilfe anzeigen"),
    ("ls", "list directory contents        ls [-a] [-l] [path]", "Verzeichnisinhalt auflisten    ls [-a] [-l] [pfad]"),
    ("cd", "change directory               cd [path | - | ~]", "Verzeichnis wechseln           cd [pfad | - | ~]"),
    ("pwd", "print working directory", "aktuelles Verzeichnis anzeigen"),
    ("cat", "print files                    cat file...", "Dateien ausgeben               cat datei..."),
    ("tree", "show a directory tree", "Verzeichnisbaum anzeigen"),
    ("mkdir", "create directories             mkdir [-p] dir...", "Verzeichnisse anlegen          mkdir [-p] verz..."),
    ("touch", "create empty files", "leere Dateien anlegen"),
    ("rm", "remove files                   rm [-r] [-f] path...", "Dateien löschen                rm [-r] [-f] pfad..."),
    ("mv", "move / rename                  mv src dst", "verschieben / umbenennen       mv quelle ziel"),
    ("cp", "copy                           cp [-r] src dst", "kopieren                       cp [-r] quelle ziel"),
    ("echo", "print text (echo hi > file)", "Text ausgeben (echo hi > datei)"),
    ("edit", "open CBEditor (lesson + hints) edit file  (alias: nano, vim)", "CBEditor öffnen (Lektion+Tipps) edit datei  (alias: nano, vim)"),
    ("run", "compile & run a file           run [file] [< input]", "Datei kompilieren & ausführen  run [datei] [< eingabe]"),
    ("submit", "check the current part         submit [file]", "aktuellen Teil prüfen          submit [datei]"),
    ("hint", "reveal the next hint for the current part", "nächsten Tipp für den aktuellen Teil zeigen"),
    ("tasks", "show all tasks and your progress", "alle Aufgaben und deinen Fortschritt zeigen"),
    ("lang", "change the language            lang [de | en]", "Sprache wechseln               lang [de | en]"),
    ("history", "show command history", "Befehlsverlauf anzeigen"),
    ("whoami", "print user name", "Benutzernamen anzeigen"),
    ("clear", "clear the screen (Ctrl+L)", "Bildschirm leeren (Strg+L)"),
    ("exit", "close CodeBetter", "CodeBetter beenden"),
];
const ALIASES: &[(&str, &str)] = &[
    ("nano", "edit"),
    ("vim", "edit"),
    ("vi", "edit"),
    ("status", "tasks"),
    ("logout", "exit"),
    ("language", "lang"),
];

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Action {
    Clear,
    Edit { path: String, name: String, content: String, task: Option<TaskContext> },
    Exit,
}

#[derive(Serialize)]
pub struct Response {
    pub output: String,
    pub prompt: String,
    pub lang: Lang,
    pub action: Option<Action>,
}

/// What CBEditor shows next to the code: the current part and its revealed hints.
#[derive(Serialize)]
pub struct TaskContext {
    pub id: String,
    pub title: String,
    pub part: usize,
    pub parts: usize,
    pub part_title: String,
    pub lesson: String,
    pub description: String,
    pub hints: Vec<String>,
    pub hints_total: usize,
    pub done: bool,
}

#[derive(Serialize)]
pub struct EditorCheck {
    pub output: String,
    pub task: Option<TaskContext>,
}

#[derive(Serialize)]
pub struct Completion {
    pub line: String,
    pub candidates: Vec<String>,
}

#[derive(Default)]
struct Out {
    text: String,
    action: Option<Action>,
}

impl Out {
    fn text(text: impl Into<String>) -> Self {
        Out { text: text.into(), action: None }
    }
    fn action(action: Action) -> Self {
        Out { text: String::new(), action: Some(action) }
    }
}

type CmdResult = Result<Out, String>;

#[derive(Debug, PartialEq)]
enum Tok {
    Word(String),
    Op(&'static str),
}

struct Invocation {
    argv: Vec<String>,
    stdin: Option<String>,
    stdout: Option<(String, bool)>,
}

pub struct Shell {
    root: PathBuf,
    progress_file: PathBuf,
    settings_file: PathBuf,
    user: String,
    home: String,
    cwd: String,
    prev_cwd: String,
    tasks: Vec<Task>,
    progress: Progress,
    lang: Lang,
}

impl Shell {
    pub fn new(root: PathBuf, data_dir: &Path, user: String, tasks: Vec<Task>) -> io::Result<Self> {
        let home = format!("/home/{user}");
        fs::create_dir_all(root.join(home.trim_start_matches('/')))?;
        let progress_file = data_dir.join("progress.json");
        let settings_file = data_dir.join("settings.json");
        let progress = Progress::load(&progress_file);
        let lang = Settings::load(&settings_file).lang;
        let shell = Shell {
            root,
            progress_file,
            settings_file,
            user,
            cwd: home.clone(),
            prev_cwd: home.clone(),
            home,
            tasks,
            progress,
            lang,
        };
        shell.sync_tasks()?;
        Ok(shell)
    }

    // ---------------------------------------------------------------- paths

    fn resolve(&self, input: &str) -> String {
        let joined = if input == "~" || input.starts_with("~/") {
            format!("{}{}", self.home, &input[1..])
        } else if input.starts_with('/') {
            input.to_string()
        } else {
            format!("{}/{}", self.cwd, input)
        };
        let mut parts: Vec<&str> = Vec::new();
        for part in joined.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                p => parts.push(p),
            }
        }
        format!("/{}", parts.join("/"))
    }

    fn real(&self, vpath: &str) -> PathBuf {
        self.root.join(vpath.trim_start_matches('/'))
    }

    fn display(&self, vpath: &str) -> String {
        if vpath == self.home {
            "~".into()
        } else if let Some(rest) = vpath.strip_prefix(&format!("{}/", self.home)) {
            format!("~/{rest}")
        } else {
            vpath.into()
        }
    }

    fn tasks_vdir(&self) -> String {
        format!("{}/{TASKS_DIR}", self.home)
    }

    fn sync_tasks(&self) -> io::Result<()> {
        tasks::materialize(&self.tasks, &self.progress, &self.real(&self.tasks_vdir()), self.lang)
    }

    /// The task whose directory contains `vpath`.
    fn task_at(&self, vpath: &str) -> Option<usize> {
        let rest = vpath.strip_prefix(&format!("{}/", self.tasks_vdir()))?;
        let id = rest.split('/').next()?;
        self.tasks.iter().position(|t| t.id == id)
    }

    fn current_task(&self) -> Option<usize> {
        self.task_at(&self.cwd)
    }

    fn context(&self, idx: usize) -> TaskContext {
        let lang = self.lang;
        let task = &self.tasks[idx];
        let p = self.progress.get(&task.id);
        let mut ctx = TaskContext {
            id: task.id.clone(),
            title: task.title.get(lang).to_string(),
            part: p.completed + 1,
            parts: task.parts.len(),
            part_title: String::new(),
            lesson: String::new(),
            description: String::new(),
            hints: vec![],
            hints_total: 0,
            done: task.is_done(&p),
        };
        match task.parts.get(p.completed) {
            Some(part) => {
                ctx.part_title = part.title.get(lang).to_string();
                ctx.lesson = part.lesson.get(lang).trim_end().to_string();
                ctx.description = part.description.get(lang).trim_end().to_string();
                ctx.hints = part.hints.iter().take(p.hints_shown).map(|h| h.get(lang).to_string()).collect();
                ctx.hints_total = part.hints.len();
            }
            None => {
                ctx.part = task.parts.len();
                ctx.part_title = tr!(lang, "complete", "geschafft");
                ctx.description = tr!(
                    lang,
                    "All lessons passed. You wrote every line of this program yourself.\n\nWhat's next:  cd .. && cat README.md",
                    "Alle Lektionen bestanden. Jede Zeile dieses Programms hast du selbst geschrieben.\n\nWie es weitergeht:  cd .. && cat README.md"
                );
            }
        }
        ctx
    }

    /// CBEditor: reveal one more hint for the task `vpath` belongs to.
    pub fn editor_hint(&mut self, vpath: &str) -> Option<TaskContext> {
        let idx = self.task_at(&self.resolve(vpath))?;
        // A failed save only loses the hint counter; the hint is still shown.
        let _ = self.reveal_hint(idx);
        Some(self.context(idx))
    }

    /// CBEditor: `submit` the open file.
    pub fn editor_check(&mut self, vpath: &str) -> EditorCheck {
        let v = self.resolve(vpath);
        let Some(idx) = self.task_at(&v) else {
            let output = tr!(
                self.lang,
                "{RED}this file doesn't belong to a task (save it inside ~/{TASKS_DIR}/<task>/){RESET}\n",
                "{RED}diese Datei gehört zu keiner Aufgabe (speichere sie in ~/{TASKS_DIR}/<aufgabe>/){RESET}\n"
            );
            return EditorCheck { output, task: None };
        };
        let output = match self.submit(idx, &v) {
            Ok(out) => out.text,
            Err(e) => format!("{RED}{e}{RESET}\n"),
        };
        EditorCheck { output, task: Some(self.context(idx)) }
    }

    fn read_text(&self, input: &str) -> Result<String, String> {
        let real = self.real(&self.resolve(input));
        if real.is_dir() {
            return Err(format!("{input}: {}", self.kind(io::ErrorKind::IsADirectory)));
        }
        fs::read_to_string(&real).map_err(|e| self.io_err(input, e))
    }

    fn kind(&self, kind: io::ErrorKind) -> String {
        use io::ErrorKind::*;
        let (en, de) = match kind {
            NotFound => ("No such file or directory", "Datei oder Verzeichnis nicht gefunden"),
            AlreadyExists => ("File exists", "Die Datei existiert bereits"),
            PermissionDenied => ("Permission denied", "Keine Berechtigung"),
            NotADirectory => ("Not a directory", "Ist kein Verzeichnis"),
            IsADirectory => ("Is a directory", "Ist ein Verzeichnis"),
            DirectoryNotEmpty => ("Directory not empty", "Verzeichnis nicht leer"),
            InvalidData => ("not a text file", "keine Textdatei"),
            _ => return kind.to_string(),
        };
        match self.lang {
            Lang::En => en.into(),
            Lang::De => de.into(),
        }
    }

    fn io_err(&self, path: &str, e: io::Error) -> String {
        format!("{path}: {}", self.kind(e.kind()))
    }

    // ------------------------------------------------------------ execution

    pub fn prompt(&self) -> String {
        format!("{}@codebetter:{}", self.user, self.display(&self.cwd))
    }

    fn respond(&self, output: String, action: Option<Action>) -> Response {
        Response { output, prompt: self.prompt(), lang: self.lang, action }
    }

    pub fn welcome(&self) -> Response {
        let done = self.tasks.iter().filter(|t| t.is_done(&self.progress.get(&t.id))).count();
        let total = self.tasks.len();
        let user = &self.user;
        let banner = format!(
            "{BOLD}{GREEN}\
  ____          _      ____       _   _\n\
 / ___|___   __| | ___| __ )  ___| |_| |_ ___ _ __\n\
| |   / _ \\ / _` |/ _ \\  _ \\ / _ \\ __| __/ _ \\ '__|\n\
| |__| (_) | (_| |  __/ |_) |  __/ |_| ||  __/ |\n\
 \\____\\___/ \\__,_|\\___|____/ \\___|\\__|\\__\\___|_|{RESET}\n\n"
        );
        let body = tr!(
            self.lang,
            "{DIM}        Learn to code. Keep the skill. Despite AI.{RESET}\n\n\
             Welcome, {BOLD}{user}{RESET}. {done} of {total} tasks completed.\n\n\
             \x20 {YELLOW}cd {TASKS_DIR}{RESET}      your tasks live here\n\
             \x20 {YELLOW}help{RESET}             list all commands\n\
             \x20 {YELLOW}lang de{RESET}          auf Deutsch umschalten\n\n",
            "{DIM}        Coden lernen und behalten. Trotz KI.{RESET}\n\n\
             Willkommen, {BOLD}{user}{RESET}. {done} von {total} Aufgaben erledigt.\n\n\
             \x20 {YELLOW}cd {TASKS_DIR}{RESET}      hier liegen deine Aufgaben\n\
             \x20 {YELLOW}help{RESET}             alle Befehle anzeigen\n\
             \x20 {YELLOW}lang en{RESET}          switch to English\n\n",
        );
        self.respond(banner + &body, None)
    }

    pub fn exec(&mut self, line: &str) -> Response {
        let out = match tokenize(line, self.lang) {
            Ok(tokens) => self.run_list(tokens),
            Err(e) => Out::text(format!("{RED}sh: {e}{RESET}\n")),
        };
        self.respond(out.text, out.action)
    }

    /// Runs `a ; b && c`, collecting the output of every command.
    fn run_list(&mut self, tokens: Vec<Tok>) -> Out {
        let mut segments: Vec<(Vec<Tok>, Option<&str>)> = vec![(vec![], None)];
        for tok in tokens {
            match tok {
                Tok::Op(op @ (";" | "&&")) => {
                    segments.last_mut().expect("never empty").1 = Some(op);
                    segments.push((vec![], None));
                }
                other => segments.last_mut().expect("never empty").0.push(other),
            }
        }

        let mut total = Out::default();
        let mut skip = false;
        for (segment, sep) in segments {
            let ok = if skip {
                false
            } else {
                match self.run_segment(segment) {
                    Ok(out) => {
                        if matches!(out.action, Some(Action::Clear)) {
                            total.text.clear();
                        }
                        total.text += &out.text;
                        total.action = out.action.or(total.action);
                        true
                    }
                    Err(e) => {
                        total.text += &format!("{RED}{e}{RESET}\n");
                        false
                    }
                }
            };
            skip = sep == Some("&&") && !ok;
        }
        total
    }

    fn run_segment(&mut self, tokens: Vec<Tok>) -> CmdResult {
        let inv = parse(tokens, self.lang)?;
        let Some(name) = inv.argv.first().cloned() else {
            return Ok(Out::default());
        };
        let stdin = match &inv.stdin {
            Some(path) => Some(self.read_text(path).map_err(|e| format!("sh: {e}"))?),
            None => None,
        };
        let name = ALIASES.iter().find(|(a, _)| *a == name).map_or(name.as_str(), |(_, c)| c).to_string();
        let mut out = self
            .dispatch(&name, &inv.argv[1..], stdin)
            .map_err(|e| format!("{}: {e}", inv.argv[0]))?;
        if let Some((path, append)) = inv.stdout {
            let real = self.real(&self.resolve(&path));
            let mut text = strip_ansi(&out.text);
            if append {
                if let Ok(old) = fs::read_to_string(&real) {
                    text = old + &text;
                }
            }
            fs::write(&real, text).map_err(|e| format!("sh: {}", self.io_err(&path, e)))?;
            out.text.clear();
        }
        Ok(out)
    }

    fn dispatch(&mut self, cmd: &str, args: &[String], stdin: Option<String>) -> CmdResult {
        match cmd {
            "help" => Ok(self.cmd_help()),
            "ls" => self.cmd_ls(args),
            "cd" => self.cmd_cd(args),
            "pwd" => Ok(Out::text(format!("{}\n", self.cwd))),
            "cat" => self.cmd_cat(args, stdin),
            "tree" => self.cmd_tree(args),
            "mkdir" => self.cmd_mkdir(args),
            "touch" => self.cmd_touch(args),
            "rm" => self.cmd_rm(args),
            "mv" => self.cmd_mv(args),
            "cp" => self.cmd_cp(args),
            "echo" => Ok(cmd_echo(args)),
            "edit" => self.cmd_edit(args),
            "run" => self.cmd_run(args, stdin),
            "submit" => self.cmd_submit(args),
            "hint" => self.cmd_hint(),
            "tasks" => Ok(self.cmd_tasks()),
            "lang" => self.cmd_lang(args),
            "whoami" => Ok(Out::text(format!("{}\n", self.user))),
            "uname" => Ok(Out::text("CodeBetter tty0 0.1.0\n")),
            "clear" => Ok(Out::action(Action::Clear)),
            "exit" => Ok(Out::action(Action::Exit)),
            "sudo" => Err(tr!(
                self.lang,
                "nice try. There is no root here, only you and your code.",
                "netter Versuch. Hier gibt es kein root, nur dich und deinen Code."
            )),
            _ => Err(tr!(self.lang, "command not found (try 'help')", "Befehl nicht gefunden (probier 'help')")),
        }
    }

    pub fn save(&self, vpath: &str, content: &str) -> Result<(), String> {
        let v = self.resolve(vpath);
        fs::write(self.real(&v), content).map_err(|e| self.io_err(&self.display(&v), e))
    }

    pub fn complete(&self, line: &str) -> Completion {
        let start = line.rfind(' ').map_or(0, |i| i + 1);
        let (head, word) = line.split_at(start);
        let first_word = head.trim().is_empty() || head.trim_end().ends_with(['&', ';']);

        let (dir_part, base) = match word.rfind('/') {
            Some(i) => word.split_at(i + 1),
            None => ("", word),
        };
        let mut matches: Vec<String> = if first_word {
            COMMANDS
                .iter()
                .map(|(c, _, _)| c.to_string())
                .filter(|c| c.starts_with(word))
                .collect()
        } else if head.trim() == "lang" {
            Lang::ALL.iter().map(|l| l.code().to_string()).filter(|c| c.starts_with(word)).collect()
        } else {
            let dir = self.real(&self.resolve(if dir_part.is_empty() { "." } else { dir_part }));
            fs::read_dir(dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter_map(|e| {
                            let name = e.file_name().to_string_lossy().into_owned();
                            let visible = !name.starts_with('.') || base.starts_with('.');
                            (visible && name.starts_with(base)).then(|| {
                                if e.path().is_dir() { name + "/" } else { name }
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        matches.sort();

        let prefix = if first_word { "" } else { dir_part };
        match matches.as_slice() {
            [] => Completion { line: line.into(), candidates: vec![] },
            [only] => {
                let space = if only.ends_with('/') { "" } else { " " };
                Completion { line: format!("{head}{prefix}{only}{space}"), candidates: vec![] }
            }
            many => {
                let common = common_prefix(many);
                let base_now = if first_word { word } else { base };
                if common.len() > base_now.len() {
                    Completion { line: format!("{head}{prefix}{common}"), candidates: vec![] }
                } else {
                    Completion { line: line.into(), candidates: many.to_vec() }
                }
            }
        }
    }

    // ------------------------------------------------------------- commands

    fn cmd_help(&self) -> Out {
        let mut s = format!("{BOLD}{}{RESET}\n", tr!(self.lang, "COMMANDS", "BEFEHLE"));
        for (name, en, de) in COMMANDS {
            let desc = if self.lang == Lang::En { en } else { de };
            s += &format!("  {YELLOW}{name:<9}{RESET}{desc}\n");
        }
        s += &tr!(
            self.lang,
            "\n{BOLD}KEYS{RESET}  Tab complete · Up/Down history · Ctrl+C cancel · Ctrl+L clear\n\
             {BOLD}SHELL{RESET} cmd > file · cmd >> file · cmd < file · a && b · a ; b\n",
            "\n{BOLD}TASTEN{RESET}  Tab vervollständigen · Hoch/Runter Verlauf · Strg+C abbrechen · Strg+L leeren\n\
             {BOLD}SHELL{RESET}   befehl > datei · befehl >> datei · befehl < datei · a && b · a ; b\n",
        );
        Out::text(s)
    }

    fn colored_name(&self, name: &str, is_dir: bool) -> String {
        if is_dir && name.ends_with(".cb") {
            format!("{BOLD}{YELLOW}{name}/{RESET}")
        } else if is_dir {
            format!("{BOLD}{BLUE}{name}/{RESET}")
        } else if name.ends_with(".md") {
            format!("{CYAN}{name}{RESET}")
        } else {
            name.to_string()
        }
    }

    fn cmd_ls(&self, args: &[String]) -> CmdResult {
        let (flags, paths) = split_flags(args);
        let (all, long) = (flags.contains('a'), flags.contains('l'));
        let targets = if paths.is_empty() { vec![".".to_string()] } else { paths };
        let mut s = String::new();
        for (i, target) in targets.iter().enumerate() {
            let real = self.real(&self.resolve(target));
            let meta = fs::metadata(&real).map_err(|e| {
                tr!(self.lang, "cannot access '{target}': {}", "Zugriff auf '{target}' nicht möglich: {}", self.kind(e.kind()))
            })?;
            if !meta.is_dir() {
                s += &format!("{target}\n");
                continue;
            }
            if targets.len() > 1 {
                s += &format!("{}{target}:\n", if i > 0 { "\n" } else { "" });
            }
            let mut entries: Vec<(String, fs::Metadata)> = fs::read_dir(&real)
                .map_err(|e| self.io_err(target, e))?
                .flatten()
                .filter_map(|e| Some((e.file_name().to_string_lossy().into_owned(), e.metadata().ok()?)))
                .filter(|(name, _)| all || !name.starts_with('.'))
                .collect();
            entries.sort_by_key(|(name, _)| name.to_lowercase());
            if long {
                for (name, meta) in &entries {
                    let kind = if meta.is_dir() { "drwxr-xr-x" } else { "-rw-r--r--" };
                    s += &format!(
                        "{kind} {} {:>7}  {}\n",
                        self.user,
                        meta.len(),
                        self.colored_name(name, meta.is_dir())
                    );
                }
            } else if !entries.is_empty() {
                let names: Vec<String> = entries.iter().map(|(n, m)| self.colored_name(n, m.is_dir())).collect();
                s += &names.join("  ");
                s += "\n";
            }
        }
        Ok(Out::text(s))
    }

    fn cmd_cd(&mut self, args: &[String]) -> CmdResult {
        let target = match args.first().map(String::as_str) {
            None => self.home.clone(),
            Some("-") => self.prev_cwd.clone(),
            Some(path) => self.resolve(path),
        };
        let real = self.real(&target);
        if !real.exists() {
            return Err(format!("{}: {}", args[0], self.kind(io::ErrorKind::NotFound)));
        }
        if !real.is_dir() {
            return Err(format!("{}: {}", args[0], self.kind(io::ErrorKind::NotADirectory)));
        }
        self.prev_cwd = std::mem::replace(&mut self.cwd, target);
        Ok(Out::default())
    }

    fn missing_operand(&self) -> String {
        tr!(self.lang, "missing operand", "Operand fehlt")
    }

    fn cmd_cat(&self, args: &[String], stdin: Option<String>) -> CmdResult {
        if args.is_empty() {
            return stdin.map(Out::text).ok_or_else(|| self.missing_operand());
        }
        let mut s = String::new();
        for arg in args {
            let text = self.read_text(arg)?;
            for line in text.lines() {
                let is_rule = !line.is_empty() && line.chars().all(|c| c == '=' || c == '-');
                let letters: String = line.chars().take_while(|c| *c != '·').filter(|c| c.is_alphabetic()).collect();
                let is_heading = !line.starts_with(' ') && !letters.is_empty() && letters == letters.to_uppercase();
                if line.starts_with('#') || is_rule || is_heading {
                    s += &format!("{BOLD}{line}{RESET}\n");
                } else {
                    s += line;
                    s += "\n";
                }
            }
        }
        Ok(Out::text(s))
    }

    fn cmd_tree(&self, args: &[String]) -> CmdResult {
        let target = args.first().map_or(".", String::as_str);
        let real = self.real(&self.resolve(target));
        if !real.is_dir() {
            return Err(format!("{target}: {}", self.kind(io::ErrorKind::NotADirectory)));
        }
        let mut s = format!("{}\n", self.colored_name(target, true));
        let (mut dirs, mut files) = (0, 0);
        self.tree_walk(&real, "", &mut s, &mut dirs, &mut files);
        s += &tr!(self.lang, "\n{dirs} directories, {files} files\n", "\n{dirs} Verzeichnisse, {files} Dateien\n");
        Ok(Out::text(s))
    }

    fn tree_walk(&self, dir: &Path, indent: &str, s: &mut String, dirs: &mut usize, files: &mut usize) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .map(|r| r.flatten().filter(|e| !e.file_name().to_string_lossy().starts_with('.')).collect())
            .unwrap_or_default();
        entries.sort_by_key(|e| e.file_name());
        for (i, entry) in entries.iter().enumerate() {
            let last = i + 1 == entries.len();
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.path().is_dir();
            *s += &format!("{indent}{}{}\n", if last { "└── " } else { "├── " }, self.colored_name(&name, is_dir));
            if is_dir {
                *dirs += 1;
                let next = format!("{indent}{}", if last { "    " } else { "│   " });
                self.tree_walk(&entry.path(), &next, s, dirs, files);
            } else {
                *files += 1;
            }
        }
    }

    fn cmd_mkdir(&self, args: &[String]) -> CmdResult {
        let (flags, paths) = split_flags(args);
        if paths.is_empty() {
            return Err(self.missing_operand());
        }
        for path in &paths {
            let real = self.real(&self.resolve(path));
            let res = if flags.contains('p') { fs::create_dir_all(&real) } else { fs::create_dir(&real) };
            res.map_err(|e| {
                tr!(self.lang, "cannot create directory '{path}': {}", "Verzeichnis '{path}' kann nicht angelegt werden: {}", self.kind(e.kind()))
            })?;
        }
        Ok(Out::default())
    }

    fn cmd_touch(&self, args: &[String]) -> CmdResult {
        if args.is_empty() {
            return Err(self.missing_operand());
        }
        for path in args {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.real(&self.resolve(path)))
                .map_err(|e| self.io_err(path, e))?;
        }
        Ok(Out::default())
    }

    fn cmd_rm(&self, args: &[String]) -> CmdResult {
        let (flags, paths) = split_flags(args);
        let (recursive, force) = (flags.contains('r') || flags.contains('R'), flags.contains('f'));
        if paths.is_empty() {
            return Err(self.missing_operand());
        }
        for path in &paths {
            let v = self.resolve(path);
            if v == "/" || v == self.home || self.home.starts_with(&format!("{v}/")) {
                return Err(tr!(self.lang, "refusing to remove '{path}'", "'{path}' wird nicht gelöscht"));
            }
            let cannot = |kind: io::ErrorKind| {
                tr!(self.lang, "cannot remove '{path}': {}", "'{path}' kann nicht gelöscht werden: {}", self.kind(kind))
            };
            let real = self.real(&v);
            let res = if real.is_dir() {
                if !recursive {
                    return Err(cannot(io::ErrorKind::IsADirectory));
                }
                fs::remove_dir_all(&real)
            } else {
                fs::remove_file(&real)
            };
            match res {
                Err(e) if !(force && e.kind() == io::ErrorKind::NotFound) => return Err(cannot(e.kind())),
                _ => {}
            }
        }
        Ok(Out::default())
    }

    /// Destination for mv/cp: copying onto a directory puts the file inside it.
    fn destination(&self, src: &str, dst: &str) -> PathBuf {
        let real = self.real(&self.resolve(dst));
        if real.is_dir() {
            let name = Path::new(&self.resolve(src)).file_name().map(|n| n.to_owned()).unwrap_or_default();
            real.join(name)
        } else {
            real
        }
    }

    fn usage(&self, en: &str, de: &str) -> String {
        tr!(self.lang, "usage: {en}", "Aufruf: {de}")
    }

    fn cmd_mv(&self, args: &[String]) -> CmdResult {
        let [src, dst] = args else {
            return Err(self.usage("mv src dst", "mv quelle ziel"));
        };
        fs::rename(self.real(&self.resolve(src)), self.destination(src, dst)).map_err(|e| {
            tr!(self.lang, "cannot move '{src}': {}", "'{src}' kann nicht verschoben werden: {}", self.kind(e.kind()))
        })?;
        Ok(Out::default())
    }

    fn cmd_cp(&self, args: &[String]) -> CmdResult {
        let (flags, paths) = split_flags(args);
        let [src, dst] = paths.as_slice() else {
            return Err(self.usage("cp [-r] src dst", "cp [-r] quelle ziel"));
        };
        let from = self.real(&self.resolve(src));
        let to = self.destination(src, dst);
        let res = if from.is_dir() {
            if !flags.contains('r') {
                return Err(tr!(
                    self.lang,
                    "-r not specified; omitting directory '{src}'",
                    "-r nicht angegeben; Verzeichnis '{src}' wird ausgelassen"
                ));
            }
            copy_dir(&from, &to)
        } else {
            fs::copy(&from, &to).map(|_| ())
        };
        res.map_err(|e| {
            tr!(self.lang, "cannot copy '{src}': {}", "'{src}' kann nicht kopiert werden: {}", self.kind(e.kind()))
        })?;
        Ok(Out::default())
    }

    fn cmd_edit(&self, args: &[String]) -> CmdResult {
        let [path] = args else {
            return Err(self.usage("edit file", "edit datei"));
        };
        let v = self.resolve(path);
        let real = self.real(&v);
        if real.is_dir() {
            return Err(format!("{path}: {}", self.kind(io::ErrorKind::IsADirectory)));
        }
        if !real.parent().is_some_and(Path::is_dir) {
            return Err(format!("{path}: {}", self.kind(io::ErrorKind::NotFound)));
        }
        let content = fs::read_to_string(&real).unwrap_or_default();
        let name = self.display(&v);
        let task = self.task_at(&v).map(|idx| self.context(idx));
        Ok(Out::action(Action::Edit { path: v, name, content, task }))
    }

    /// The solution file for `submit`/`run` when none is given.
    fn default_source(&self) -> Result<String, String> {
        let task = self.current_task().ok_or_else(|| {
            tr!(
                self.lang,
                "no file given, and you're not inside a task directory",
                "keine Datei angegeben, und du bist in keinem Aufgabenverzeichnis"
            )
        })?;
        let task = &self.tasks[task];
        if self.real(&self.resolve(&task.starter_file)).exists() {
            return Ok(task.starter_file.clone());
        }
        ["main.rs", "main.py", "main.js", "main.c"]
            .into_iter()
            .find(|f| self.real(&self.resolve(f)).is_file())
            .map(String::from)
            .ok_or_else(|| {
                tr!(
                    self.lang,
                    "no solution file found (expected {})",
                    "keine Lösungsdatei gefunden (erwartet: {})",
                    task.starter_file
                )
            })
    }

    fn prepare(&self, file: &str, s: &mut String) -> Result<runner::Program, String> {
        let real = self.real(&self.resolve(file));
        if !real.is_file() {
            return Err(format!("{file}: {}", self.kind(io::ErrorKind::NotFound)));
        }
        *s += &tr!(self.lang, "{DIM}» building {file} ...{RESET}\n", "{DIM}» baue {file} ...{RESET}\n");
        match runner::prepare(&real, self.lang) {
            Ok(program) => {
                *s += &program.compile_log;
                Ok(program)
            }
            Err(log) => {
                *s += &log;
                *s += &tr!(self.lang, "{RED}{BOLD}✗ build failed{RESET}\n", "{RED}{BOLD}✗ Build fehlgeschlagen{RESET}\n");
                Err(String::new())
            }
        }
    }

    fn time_limit(&self) -> String {
        let secs = TEST_TIMEOUT.as_secs();
        tr!(self.lang, "time limit of {secs}s exceeded", "Zeitlimit von {secs}s überschritten")
    }

    fn cmd_run(&self, args: &[String], stdin: Option<String>) -> CmdResult {
        let file = match args.first() {
            Some(f) => f.clone(),
            None => self.default_source()?,
        };
        let mut s = String::new();
        let Ok(program) = self.prepare(&file, &mut s) else {
            return Ok(Out::text(s));
        };
        let res = program.run(stdin.as_deref().unwrap_or(""), TEST_TIMEOUT, self.lang)?;
        s += &res.stdout;
        if !res.stdout.is_empty() && !res.stdout.ends_with('\n') {
            s += &format!("{DIM}%{RESET}\n");
        }
        if !res.stderr.is_empty() {
            s += &format!("{RED}{}{RESET}", res.stderr);
        }
        let status = if res.timed_out {
            format!("{RED}{}: {}{RESET}", tr!(self.lang, "killed", "abgebrochen"), self.time_limit())
        } else {
            format!("exit {}", res.code.map_or("?".into(), |c| c.to_string()))
        };
        s += &format!("{DIM}[{status}{DIM} · {} ms]{RESET}\n", res.elapsed.as_millis());
        Ok(Out::text(s))
    }

    fn cmd_submit(&mut self, args: &[String]) -> CmdResult {
        let lang = self.lang;
        let idx = self.current_task().ok_or_else(|| {
            tr!(lang, "go into a task first:  cd ~/{TASKS_DIR}/<task>", "geh zuerst in eine Aufgabe:  cd ~/{TASKS_DIR}/<aufgabe>")
        })?;
        let file = match args.first() {
            Some(f) => f.clone(),
            None => self.default_source()?,
        };
        self.submit(idx, &file)
    }

    fn submit(&mut self, idx: usize, file: &str) -> CmdResult {
        let lang = self.lang;
        let task = &self.tasks[idx];
        let progress = self.progress.get(&task.id);
        if task.is_done(&progress) {
            let id = &task.id;
            return Ok(Out::text(tr!(
                lang,
                "{GREEN}{id} is already complete. Try the next task: cd ../{RESET}\n",
                "{GREEN}{id} ist schon erledigt. Weiter mit der nächsten Aufgabe: cd ../{RESET}\n"
            )));
        }
        let part_idx = progress.completed;
        let part = &task.parts[part_idx];

        let mut s = format!(
            "{BOLD}SUBMIT{RESET} {} · {} {}/{} · {}\n",
            task.id,
            tr!(lang, "lesson", "Lektion"),
            part_idx + 1,
            task.parts.len(),
            part.title.get(lang)
        );
        let Ok(program) = self.prepare(file, &mut s) else {
            s += &tr!(lang, "{RED}submission rejected{RESET}\n", "{RED}Abgabe abgelehnt{RESET}\n");
            return Ok(Out::text(s));
        };

        let mut failure: Option<String> = None;
        let mut passed = 0;
        for (i, test) in part.tests.iter().enumerate() {
            let input = test.input.get(lang);
            let res = program.run(input, TEST_TIMEOUT, lang)?;
            let label = if test.hidden { tr!(lang, " (hidden)", " (versteckt)") } else { String::new() };
            let got = runner::normalize(&res.stdout);
            let want = runner::normalize(test.expected.get(lang));
            let problem = if res.timed_out {
                Some(tr!(lang, "{}. Endless loop?", "{}. Endlosschleife?", self.time_limit()))
            } else if res.code != Some(0) {
                let code = res.code.map_or("?".into(), |c| c.to_string());
                Some(tr!(
                    lang,
                    "program crashed (exit {code})\n{}",
                    "Programm abgestürzt (exit {code})\n{}",
                    res.stderr.trim_end()
                ))
            } else if got != want {
                Some(tr!(lang, "wrong output", "falsche Ausgabe"))
            } else {
                None
            };
            let n = i + 1;
            match problem {
                None => {
                    passed += 1;
                    s += &format!("  test {n}{label:<12} {GREEN}ok{RESET}\n");
                }
                Some(why) => {
                    s += &format!("  test {n}{label:<12} {RED}{}{RESET}\n", tr!(lang, "FAILED", "FEHLGESCHLAGEN"));
                    if failure.is_none() {
                        let mut f = tr!(lang, "\n{RED}{BOLD}test {n} failed:{RESET} {why}\n", "\n{RED}{BOLD}Test {n} fehlgeschlagen:{RESET} {why}\n");
                        if test.hidden {
                            f += &tr!(
                                lang,
                                "{DIM}  this test is hidden. Think about edge cases the examples don't show.{RESET}\n",
                                "{DIM}  dieser Test ist versteckt. Denk an Randfälle, die die Beispiele nicht zeigen.{RESET}\n"
                            );
                        } else {
                            let (l_in, l_want, l_got) = match lang {
                                Lang::En => ("input", "expected", "got"),
                                Lang::De => ("Eingabe", "erwartet", "erhalten"),
                            };
                            f += &format!("{DIM}  {l_in}:{RESET}\n{}", indent_block(input, lang));
                            f += &format!("{DIM}  {l_want}:{RESET}\n{}", indent_block(&want, lang));
                            f += &format!("{DIM}  {l_got}:{RESET}\n{}", indent_block(&got, lang));
                        }
                        failure = Some(f);
                    }
                }
            }
        }

        let total = part.tests.len();
        if let Some(f) = failure {
            s += &f;
            s += &tr!(
                lang,
                "\n{RED}{passed}/{total} passed. Not quite yet.{RESET} {DIM}Stuck? type 'hint'.{RESET}\n",
                "\n{RED}{passed}/{total} bestanden. Noch nicht ganz.{RESET} {DIM}Hängst du fest? Tippe 'hint'.{RESET}\n"
            );
            return Ok(Out::text(s));
        }

        let source = self.read_text(file).unwrap_or_default();
        let comment = if file.ends_with(".py") { "#" } else { "//" };
        let missing = part.missing_requirements(&source, comment);
        if !missing.is_empty() {
            s += &tr!(
                lang,
                "\n{YELLOW}{BOLD}Output is correct, but the code doesn't use what this lesson is about yet:{RESET}\n",
                "\n{YELLOW}{BOLD}Die Ausgabe stimmt, aber der Code benutzt noch nicht, worum es in dieser Lektion geht:{RESET}\n"
            );
            for r in missing {
                s += &format!("  {YELLOW}»{RESET} {}\n", r.message.get(lang));
            }
            return Ok(Out::text(s));
        }

        let task_id = task.id.clone();
        let total_parts = task.parts.len();
        let p = self.progress.get_mut(&task_id);
        p.completed += 1;
        p.hints_shown = 0;
        let completed = p.completed;
        self.progress
            .save(&self.progress_file)
            .map_err(|e| tr!(lang, "could not save progress: {e}", "Fortschritt konnte nicht gespeichert werden: {e}"))?;
        self.sync_tasks()
            .map_err(|e| tr!(lang, "could not unlock next part: {e}", "nächster Teil konnte nicht freigeschaltet werden: {e}"))?;

        let bar = tasks::progress_bar(completed, total_parts, 20);
        s += &tr!(lang, "\n{GREEN}{BOLD}✓ all {total} tests passed{RESET}  {bar}\n", "\n{GREEN}{BOLD}✓ alle {total} Tests bestanden{RESET}  {bar}\n");
        if completed < total_parts {
            let next = tasks::part_file(completed);
            s += &tr!(
                lang,
                "{YELLOW}» next lesson unlocked:{RESET} {BOLD}cat {next}{RESET}  {DIM}(CBEditor shows it right away){RESET}\n",
                "{YELLOW}» nächste Lektion freigeschaltet:{RESET} {BOLD}cat {next}{RESET}  {DIM}(CBEditor zeigt sie sofort an){RESET}\n"
            );
        } else {
            s += &tr!(
                lang,
                "\n{GREEN}{BOLD}  ★ COURSE COMPLETE ★{RESET}\n  {GREEN}You wrote every line of it yourself. That's the point.{RESET}\n  {DIM}next:  cd .. && cat README.md{RESET}\n",
                "\n{GREEN}{BOLD}  ★ KURS GESCHAFFT ★{RESET}\n  {GREEN}Jede Zeile davon hast du selbst geschrieben. Genau darum geht's.{RESET}\n  {DIM}weiter:  cd .. && cat README.md{RESET}\n"
            );
        }
        Ok(Out::text(s))
    }

    fn reveal_hint(&mut self, idx: usize) -> Result<(), String> {
        let task = &self.tasks[idx];
        let p = self.progress.get(&task.id);
        let Some(part) = task.parts.get(p.completed) else {
            return Ok(());
        };
        if p.hints_shown >= part.hints.len() {
            return Ok(());
        }
        let id = task.id.clone();
        self.progress.get_mut(&id).hints_shown += 1;
        self.progress.save(&self.progress_file).map_err(|e| e.to_string())
    }

    fn cmd_hint(&mut self) -> CmdResult {
        let lang = self.lang;
        let idx = self.current_task().ok_or_else(|| {
            tr!(lang, "hints are available inside a task directory", "Tipps gibt es nur in einem Aufgabenverzeichnis")
        })?;
        self.reveal_hint(idx)?;
        let task = &self.tasks[idx];
        let p = self.progress.get(&task.id);
        if task.is_done(&p) {
            return Ok(Out::text(tr!(lang, "This task is complete. No hints needed!\n", "Diese Aufgabe ist erledigt. Keine Tipps nötig!\n")));
        }
        let hints = &task.parts[p.completed].hints;
        if hints.is_empty() {
            return Ok(Out::text(tr!(
                lang,
                "No hints for this part. Re-read the description carefully.\n",
                "Für diesen Teil gibt es keine Tipps. Lies die Beschreibung noch einmal genau.\n"
            )));
        }
        let word = tr!(lang, "hint", "Tipp");
        let mut s = String::new();
        for (i, hint) in hints.iter().enumerate().take(p.hints_shown) {
            s += &format!("{YELLOW}{word} {}/{}:{RESET} {}\n", i + 1, hints.len(), hint.get(lang));
        }
        if p.hints_shown == hints.len() {
            s += &tr!(lang, "{DIM}That's all the hints. You've got this.{RESET}\n", "{DIM}Das waren alle Tipps. Du schaffst das.{RESET}\n");
        }
        Ok(Out::text(s))
    }

    fn cmd_tasks(&self) -> Out {
        let mut s = format!("{BOLD}{}{RESET}  {DIM}~/{TASKS_DIR}{RESET}\n\n", tr!(self.lang, "TASKS", "AUFGABEN"));
        for task in &self.tasks {
            let p = self.progress.get(&task.id);
            let color = if task.is_done(&p) { GREEN } else if p.completed > 0 { YELLOW } else { "" };
            s += &format!(
                "  {color}{} {}/{}{RESET}  {:<16} {}\n",
                tasks::progress_bar(p.completed, task.parts.len(), 12),
                p.completed,
                task.parts.len(),
                task.id,
                task.title.get(self.lang)
            );
        }
        Out::text(s)
    }

    fn cmd_lang(&mut self, args: &[String]) -> CmdResult {
        let Some(arg) = args.first() else {
            let mut s = tr!(self.lang, "language: {}\n\n", "Sprache: {}\n\n", self.lang.name());
            for l in Lang::ALL {
                let mark = if l == self.lang { "*" } else { " " };
                s += &format!("  {mark} {YELLOW}{}{RESET}  {}\n", l.code(), l.name());
            }
            s += &tr!(self.lang, "\nchange with:  lang de\n", "\nwechseln mit:  lang en\n");
            return Ok(Out::text(s));
        };
        let lang = Lang::parse(arg).ok_or_else(|| {
            tr!(self.lang, "unknown language '{arg}' (available: de, en)", "unbekannte Sprache '{arg}' (verfügbar: de, en)")
        })?;
        self.lang = lang;
        Settings { lang }.save(&self.settings_file).map_err(|e| e.to_string())?;
        self.sync_tasks().map_err(|e| e.to_string())?;
        Ok(Out::text(tr!(
            lang,
            "{GREEN}language set to English.{RESET} {DIM}Task files were rewritten; your own code is untouched.{RESET}\n",
            "{GREEN}Sprache auf Deutsch gestellt.{RESET} {DIM}Aufgabendateien wurden neu geschrieben, dein eigener Code bleibt unverändert.{RESET}\n"
        )))
    }
}

// ---------------------------------------------------------------- helpers

fn cmd_echo(args: &[String]) -> Out {
    match args.first().map(String::as_str) {
        Some("-n") => Out::text(args[1..].join(" ")),
        _ => Out::text(format!("{}\n", args.join(" "))),
    }
}

fn tokenize(line: &str, lang: Lang) -> Result<Vec<Tok>, String> {
    fn flush(toks: &mut Vec<Tok>, cur: &mut String, in_word: &mut bool) {
        if *in_word {
            toks.push(Tok::Word(std::mem::take(cur)));
            *in_word = false;
        }
    }
    let unterminated = || tr!(lang, "unterminated quote", "Anführungszeichen nicht geschlossen");
    let mut toks = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => return Err(unterminated()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => cur.extend(chars.next()),
                        Some(ch) => cur.push(ch),
                        None => return Err(unterminated()),
                    }
                }
            }
            '\\' => {
                in_word = true;
                cur.extend(chars.next());
            }
            c if c.is_whitespace() => flush(&mut toks, &mut cur, &mut in_word),
            '>' | '<' | ';' | '&' => {
                flush(&mut toks, &mut cur, &mut in_word);
                let op = match c {
                    '>' if chars.peek() == Some(&'>') => {
                        chars.next();
                        ">>"
                    }
                    '>' => ">",
                    '<' => "<",
                    ';' => ";",
                    '&' if chars.peek() == Some(&'&') => {
                        chars.next();
                        "&&"
                    }
                    _ => {
                        return Err(tr!(
                            lang,
                            "background jobs are not supported",
                            "Hintergrundjobs werden nicht unterstützt"
                        ))
                    }
                };
                toks.push(Tok::Op(op));
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    flush(&mut toks, &mut cur, &mut in_word);
    Ok(toks)
}

fn parse(tokens: Vec<Tok>, lang: Lang) -> Result<Invocation, String> {
    let mut inv = Invocation { argv: vec![], stdin: None, stdout: None };
    let mut tokens = tokens.into_iter();
    while let Some(tok) = tokens.next() {
        match tok {
            Tok::Word(w) => inv.argv.push(w),
            Tok::Op(op) => {
                let Some(Tok::Word(target)) = tokens.next() else {
                    return Err(tr!(lang, "sh: syntax error near '{op}'", "sh: Syntaxfehler bei '{op}'"));
                };
                match op {
                    "<" => inv.stdin = Some(target),
                    ">" => inv.stdout = Some((target, false)),
                    _ => inv.stdout = Some((target, true)),
                }
            }
        }
    }
    Ok(inv)
}

/// Splits `-la`-style flags from operands.
fn split_flags(args: &[String]) -> (String, Vec<String>) {
    let mut flags = String::new();
    let mut rest = Vec::new();
    for arg in args {
        match arg.strip_prefix('-') {
            Some(f) if !f.is_empty() => flags += f,
            _ => rest.push(arg.clone()),
        }
    }
    (flags, rest)
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn common_prefix(words: &[String]) -> String {
    let first = &words[0];
    let mut len = first.len();
    for w in &words[1..] {
        len = len.min(first.bytes().zip(w.bytes()).take_while(|(a, b)| a == b).count());
    }
    while !first.is_char_boundary(len) {
        len -= 1;
    }
    first[..len].to_string()
}

fn indent_block(text: &str, lang: Lang) -> String {
    if text.is_empty() {
        return format!("    {DIM}{}{RESET}\n", tr!(lang, "(empty)", "(leer)"));
    }
    text.lines().map(|l| format!("    {l}\n")).collect()
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const COURSE: &str = "01_rust_basics";

    fn words(line: &str) -> Vec<Tok> {
        tokenize(line, Lang::En).unwrap()
    }

    #[test]
    fn tokenizes_quotes_and_operators() {
        assert_eq!(
            words(r#"echo "a b" c\ d>>out"#),
            vec![
                Tok::Word("echo".into()),
                Tok::Word("a b".into()),
                Tok::Word("c d".into()),
                Tok::Op(">>"),
                Tok::Word("out".into()),
            ]
        );
        assert_eq!(words("echo ''"), vec![Tok::Word("echo".into()), Tok::Word("".into())]);
        assert!(tokenize("echo 'oops", Lang::En).is_err());
    }

    #[test]
    fn strips_ansi() {
        assert_eq!(strip_ansi(&format!("{RED}x{RESET}y")), "xy");
    }

    fn temp_shell(name: &str) -> (Shell, PathBuf) {
        let dir = std::env::temp_dir().join(format!("codebetter-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let shell = Shell::new(dir.join("fs"), &dir, "dev".into(), tasks::load_catalog()).unwrap();
        (shell, dir)
    }

    fn solution(n: usize, lang: Lang) -> String {
        let path = format!("{}/tasks/solutions/{COURSE}/part{n}.rs", env!("CARGO_MANIFEST_DIR"));
        let de = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        if lang == Lang::De {
            return de;
        }
        // The reference solutions are German; translate the texts they print.
        [
            ("Unbekannter Artikel", "Unknown item"),
            ("Ungültige Menge", "Invalid quantity"),
            ("Kein Rabatt", "No discount"),
            ("Zu zahlen", "To pay"),
            ("\"Artikel: ", "\"Item: "),
            ("\"Preis: ", "\"Price: "),
            ("\"Menge: ", "\"Quantity: "),
            ("\"Summe: ", "\"Total: "),
            ("\"Rabatt: ", "\"Discount: "),
            ("\"Posten: ", "\"Items: "),
            ("\"Apfel\"", "\"Apple\""),
            ("\"Brot\"", "\"Bread\""),
            ("\"Milch\"", "\"Milk\""),
        ]
        .iter()
        .fold(de, |code, (from, to)| code.replace(from, to))
    }

    #[test]
    fn paths_stay_inside_the_sandbox() {
        let (mut sh, dir) = temp_shell("paths");
        sh.exec("lang en");
        assert_eq!(sh.resolve("../../../../etc"), "/etc");
        sh.exec(&format!("cd Tasks.cb/{COURSE} && cd ../.."));
        assert_eq!(sh.prompt(), "dev@codebetter:~");
        assert!(sh.exec("cd nope").output.contains("No such file"));
        assert!(sh.exec("rm -r ~").output.contains("refusing"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn aliases_are_only_for_editors_and_english_synonyms() {
        let (mut sh, dir) = temp_shell("aliases");
        sh.exec(&format!("cd Tasks.cb/{COURSE}"));
        assert!(matches!(sh.exec("nano main.rs").action, Some(Action::Edit { .. })));
        assert!(matches!(sh.exec("vim main.rs").action, Some(Action::Edit { .. })));
        assert!(sh.exec("abgeben").output.contains("Befehl nicht gefunden"));
        assert!(sh.exec("sprache en").output.contains("Befehl nicht gefunden"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn lessons_unlock_one_by_one() {
        let (mut sh, dir) = temp_shell("submit");
        sh.exec("lang en");
        sh.exec(&format!("cd Tasks.cb/{COURSE}"));
        let part1 = sh.exec("cat part1.md").output;
        assert!(part1.contains("LEARN") && part1.contains("YOUR TASK"), "{part1}");
        assert!(sh.exec("cat part2.md").output.contains("No such file"));

        sh.save("main.rs", r#"fn main() { println!("=== CB-Shop ==="); }"#).unwrap();
        let out = sh.exec("submit").output;
        assert!(out.contains("wrong output"), "{out}");

        // Right output, but without the concept of the lesson.
        sh.save("main.rs", r#"fn main() { print!("=== CB Shop ===\n"); }  // println!"#).unwrap();
        let out = sh.exec("submit").output;
        assert!(out.contains("doesn't use") && out.contains("println!"), "{out}");
        assert!(sh.exec("cat part2.md").output.contains("No such file"));

        sh.save("main.rs", &solution(1, Lang::En)).unwrap();
        let out = sh.exec("submit").output;
        assert!(out.contains("all 1 tests passed"), "{out}");
        assert!(sh.exec("cat part2.md").output.contains("Variables"));
        assert!(sh.exec("hint").output.contains("hint 1/2"));
        let _ = fs::remove_dir_all(dir);
    }

    /// Every reference solution passes its lesson, in every language.
    #[test]
    fn whole_course_is_solvable() {
        for lang in Lang::ALL {
            let (mut sh, dir) = temp_shell(&format!("course-{}", lang.code()));
            sh.exec(&format!("lang {}", lang.code()));
            sh.exec(&format!("cd Tasks.cb/{COURSE}"));
            let lessons = sh.tasks[0].parts.len();
            for n in 1..=lessons {
                sh.save("main.rs", &solution(n, lang)).unwrap();
                let out = sh.exec("submit").output;
                let passed = tr!(lang, "tests passed", "Tests bestanden");
                assert!(out.contains(&passed), "lesson {n} ({}):\n{out}", lang.code());
            }
            assert!(sh.exec("tasks").output.contains(&format!("{lessons}/{lessons}")));
            let _ = fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn german_is_default_and_switchable() {
        let (mut sh, dir) = temp_shell("lang");
        assert_eq!(sh.lang, Lang::De);
        sh.exec(&format!("cd Tasks.cb/{COURSE}"));
        assert!(sh.exec("cat part1.md").output.contains("DEINE AUFGABE"));
        sh.save("main.rs", &solution(1, Lang::De)).unwrap();
        assert!(sh.exec("submit").output.contains("alle 1 Tests bestanden"));

        assert!(sh.exec("lang en").output.contains("English"));
        assert!(sh.exec("cat part2.md").output.contains("Item: Apple"));
        assert!(sh.exec("lang xx").output.contains("unknown language"));

        let ctx = sh.editor_hint("main.rs").unwrap();
        assert_eq!((ctx.part, ctx.hints.len()), (2, 1));
        assert!(ctx.lesson.contains("let"));
        let check = sh.editor_check("main.rs");
        assert!(check.output.contains("FAILED"), "{}", check.output);
        assert!(sh.editor_check("/home/dev/x.rs").task.is_none());

        let reloaded = Shell::new(dir.join("fs"), &dir, "dev".into(), tasks::load_catalog()).unwrap();
        assert_eq!(reloaded.lang, Lang::En);
        let _ = fs::remove_dir_all(dir);
    }
}
