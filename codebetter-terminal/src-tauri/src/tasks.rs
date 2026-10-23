//! Task catalog, learner progress and the files they produce inside ~/Tasks.cb.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::i18n::{Lang, Text};
use crate::tr;

const CATALOG: &str = include_str!("../tasks/tasks.toml");

#[derive(Deserialize)]
struct Catalog {
    #[serde(rename = "task")]
    tasks: Vec<Task>,
}

#[derive(Deserialize)]
pub struct Task {
    pub id: String,
    pub title: Text,
    pub summary: Text,
    pub starter_file: String,
    pub starter_code: Text,
    #[serde(rename = "part")]
    pub parts: Vec<Part>,
}

#[derive(Deserialize)]
pub struct Part {
    pub title: Text,
    /// Theory shown before the exercise.
    #[serde(default)]
    pub lesson: Text,
    /// The exercise.
    pub description: Text,
    #[serde(default)]
    pub hints: Vec<Text>,
    #[serde(default, rename = "require")]
    pub requires: Vec<Requirement>,
    #[serde(rename = "test")]
    pub tests: Vec<TestCase>,
}

/// Something the source code must contain, e.g. `let mut `.
#[derive(Deserialize)]
pub struct Requirement {
    pub code: String,
    pub message: Text,
}

impl Part {
    /// Requirements the source doesn't meet. Code and requirement are
    /// compared as tokens (see `tokens`), so spacing never matters, and
    /// comments (`comment` to the line end, and `/* */` outside Python)
    /// don't count.
    pub fn missing_requirements(&self, source: &str, comment: &str) -> Vec<&Requirement> {
        let code = strip_comments(source, comment);
        let code = tokens(&code);
        self.requires
            .iter()
            .filter(|r| {
                let want = tokens(&r.code);
                !want.is_empty() && !code.windows(want.len()).any(|w| w == want)
            })
            .collect()
    }
}

/// Code as tokens: words (names, keywords, numbers) and every other char
/// on its own. `int*p`, `int *p` and `int * p` are all `int` `*` `p`, and
/// a word only matches whole: `for` is not in `format`.
fn tokens(code: &str) -> Vec<&str> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = Vec::new();
    let mut chars = code.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        let mut end = start + c.len_utf8();
        if is_word(c) {
            while let Some(&(i, d)) = chars.peek() {
                if !is_word(d) {
                    break;
                }
                end = i + d.len_utf8();
                chars.next();
            }
        }
        out.push(&code[start..end]);
    }
    out
}

/// The code without comments; line breaks stay.
fn strip_comments(source: &str, comment: &str) -> String {
    let mut code = source.to_owned();
    if comment == "//" {
        while let Some(start) = code.find("/*") {
            let end = code[start + 2..].find("*/").map_or(code.len(), |i| start + 2 + i + 2);
            let breaks = code[start..end].matches('\n').count();
            code.replace_range(start..end, &"\n".repeat(breaks));
        }
    }
    code.lines().map(|l| l.find(comment).map_or(l, |i| &l[..i])).collect::<Vec<_>>().join("\n")
}

#[derive(Deserialize)]
pub struct TestCase {
    #[serde(default)]
    pub input: Text,
    pub expected: Text,
    #[serde(default)]
    pub hidden: bool,
}

pub fn load_catalog() -> Vec<Task> {
    toml::from_str::<Catalog>(CATALOG)
        .expect("tasks/tasks.toml is invalid")
        .tasks
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct TaskProgress {
    /// Number of parts passed so far.
    pub completed: usize,
    /// Hints revealed for the current part.
    pub hints_shown: usize,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Progress {
    #[serde(default)]
    tasks: BTreeMap<String, TaskProgress>,
}

impl Progress {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(path, json)
    }

    pub fn get(&self, id: &str) -> TaskProgress {
        self.tasks.get(id).cloned().unwrap_or_default()
    }

    pub fn get_mut(&mut self, id: &str) -> &mut TaskProgress {
        self.tasks.entry(id.to_string()).or_default()
    }
}

impl Task {
    /// Number of parts whose description file is visible.
    pub fn unlocked(&self, progress: &TaskProgress) -> usize {
        (progress.completed + 1).min(self.parts.len())
    }

    pub fn is_done(&self, progress: &TaskProgress) -> bool {
        progress.completed >= self.parts.len()
    }
}

pub fn part_file(index: usize) -> String {
    format!("part{}.md", index + 1)
}

/// Writes the task tree: index, per-task README, starter code (never
/// overwritten) and the descriptions of every unlocked part.
pub fn materialize(tasks: &[Task], progress: &Progress, dir: &Path, lang: Lang) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join("README.md"), index_readme(tasks, progress, lang))?;
    for task in tasks {
        let task_dir = dir.join(&task.id);
        fs::create_dir_all(&task_dir)?;
        let p = progress.get(&task.id);
        fs::write(task_dir.join("README.md"), task_readme(task, &p, lang))?;
        let starter = task_dir.join(&task.starter_file);
        if !starter.exists() {
            fs::write(starter, task.starter_code.get(lang).trim_start())?;
        }
        for (i, part) in task.parts.iter().enumerate().take(task.unlocked(&p)) {
            fs::write(task_dir.join(part_file(i)), part_text(task, i, part, lang))?;
        }
    }
    Ok(())
}

pub fn progress_bar(done: usize, total: usize, width: usize) -> String {
    let filled = if total == 0 { width } else { done * width / total };
    format!("[{}{}]", "#".repeat(filled), ".".repeat(width - filled))
}

fn underline(heading: &str) -> String {
    format!("{heading}\n{}", "=".repeat(heading.chars().count()))
}

fn index_readme(tasks: &[Task], progress: &Progress, lang: Lang) -> String {
    let mut s = underline("TASKS.CB");
    s += &tr!(
        lang,
        "\n\nEvery directory here is one course. A course is one program that\nyou extend lesson by lesson; each submit unlocks the next lesson.\n\n",
        "\n\nJedes Verzeichnis hier ist ein Kurs. Ein Kurs ist ein Programm, das\ndu Lektion für Lektion erweiterst; jedes submit schaltet die nächste\nLektion frei.\n\n",
    );
    for task in tasks {
        let p = progress.get(&task.id);
        let done = if task.is_done(&p) { tr!(lang, "(done)", "(fertig)") } else { String::new() };
        s += &format!(
            "  {} {:<16} {}  {done}\n",
            progress_bar(p.completed, task.parts.len(), 10),
            task.id,
            task.title.get(lang),
        );
    }
    if let Some(first) = tasks.first() {
        let id = &first.id;
        s += &tr!(lang, "\nStart with:  cd {id} && edit main.rs\n", "\nLos geht's:  cd {id} && edit main.rs\n");
    }
    s
}

fn task_readme(task: &Task, p: &TaskProgress, lang: Lang) -> String {
    let mut s = underline(&format!("{} · {}", task.id, task.title.get(lang)));
    s += &format!("\n\n{}\n\n{}\n", task.summary.get(lang), tr!(lang, "LESSONS", "LEKTIONEN"));
    for (i, part) in task.parts.iter().enumerate() {
        let line = if i < p.completed {
            format!("  [x] {}. {:<24} {}\n", i + 1, part.title.get(lang), part_file(i))
        } else if i == p.completed {
            let current = tr!(lang, "<- current", "<- aktuell");
            format!("  [>] {}. {:<24} {}   {current}\n", i + 1, part.title.get(lang), part_file(i))
        } else {
            format!("  [ ] {}. {:<24} {}\n", i + 1, "?????", tr!(lang, "(locked)", "(gesperrt)"))
        };
        s += &line;
    }
    let part = part_file(p.completed.min(task.parts.len() - 1));
    let file = &task.starter_file;
    s += &tr!(
        lang,
        "\nWORKFLOW\n  cat {part}       read the current lesson\n  edit {file}        write your solution in CBEditor (F5 check, F2 hint)\n  run {file}         try it out (run {file} < input.txt)\n  submit              check it against the tests\n  hint                stuck? get a nudge, not the answer\n",
        "\nABLAUF\n  cat {part}       die aktuelle Lektion lesen\n  edit {file}        Lösung in CBEditor schreiben (F5 prüfen, F2 Tipp)\n  run {file}         ausprobieren (run {file} < eingabe.txt)\n  submit              gegen die Tests prüfen\n  hint                hängst du fest? Ein Denkanstoß, nicht die Lösung\n",
    );
    s
}

fn part_text(task: &Task, index: usize, part: &Part, lang: Lang) -> String {
    let heading = tr!(lang, "LESSON {}/{} · {}", "LEKTION {}/{} · {}", index + 1, task.parts.len(), part.title.get(lang));
    let hidden = part.tests.iter().filter(|t| t.hidden).count();
    let mut s = underline(&heading);
    let lesson = part.lesson.get(lang).trim_end();
    if !lesson.is_empty() {
        let title = tr!(lang, "LEARN", "LERNEN");
        s += &format!("\n\n{title}\n{}\n{lesson}", "-".repeat(title.chars().count()));
    }
    let title = tr!(lang, "YOUR TASK", "DEINE AUFGABE");
    s += &format!("\n\n{title}\n{}\n{}\n", "-".repeat(title.chars().count()), part.description.get(lang).trim_end());
    s += &tr!(lang, "\nTESTS  {} ({} hidden)\n", "\nTESTS  {} ({} versteckt)\n", part.tests.len(), hidden);
    s += &tr!(lang, "\nWhen you're done:  submit   (or F5 in CBEditor)\n", "\nWenn du fertig bist:  submit   (oder F5 im CBEditor)\n");
    s
}
