//! The start screen, like the boot of the CodeBetter terminal app: the
//! CB-BIOS lines, the CodeBetter banner, then the courses to pick from.
//!
//! ```text
//!  ▓▒░ CBEditor                                         ~/Tasks.cb   top bar
//!    ____          _      ____       _   _                           banner
//!          Coden lernen und behalten. Trotz KI.
//!   ══ SPRACHE & PROJEKT ═══════════════════════
//!   ▶ 1  Rust     Rust-Grundlagen: CB Shop    [###.......]  3/10       list
//!     2  Java     Java-Grundlagen: CB Kino    [..........]  neu
//!     +  Neue Aufgabe mit KI erstellen   KI:connect.nrw        actions
//!     ⇩  Aufgabe importieren (.cbtask)
//!   Rust von null an. In 10 Lektionen ...                        summary
//!   Ordner: ~/Tasks.cb/01_rust_basics/main.rs
//!  ↑↓ Auswählen  Enter Öffnen  L English  Esc Beenden            keys
//! ```

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveTo};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::queue;
use crossterm::terminal;

use crate::ai;
use crate::catalog;
use crate::check;
use crate::draw::{center, put, put_line};
use crate::i18n::Lang;
use crate::lesson::{self, Course, Line, Span};
use crate::theme::{self, Paint};
use crate::tr;
use crate::workspace;

/// Pause between two boot lines, as in the app.
const BOOT_STEP: Duration = Duration::from_millis(110);

const BANNER: [&str; 5] = [
    r"  ____          _      ____       _   _",
    r" / ___|___   __| | ___| __ )  ___| |_| |_ ___ _ __",
    r"| |   / _ \ / _` |/ _ \  _ \ / _ \ __| __/ _ \ '__|",
    r"| |__| (_) | (_| |  __/ |_) |  __/ |_| ||  __/ |",
    r" \____\___/ \__,_|\___|____/ \___|\__|\__\___|_|",
];

fn boot_lines(lang: Lang) -> [&'static str; 5] {
    match lang {
        Lang::De => [
            "CB-BIOS v0.1  (C) 1987 CodeBetter Systems",
            "Speichertest ...... 640K OK",
            "Suche Gehirn ...... gefunden",
            "Autopilot (KI) .... deaktiviert",
            "Mounte Home ....... ok",
        ],
        Lang::En => [
            "CB-BIOS v0.1  (C) 1987 CodeBetter Systems",
            "Memory test ........ 640K OK",
            "Detecting brain .... found",
            "Disabling autopilot  done",
            "Mounting home ...... ok",
        ],
    }
}

/// One turn of the spinner while the AI works.
const SPIN_STEP: Duration = Duration::from_millis(100);
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// The AI is writing a course in the background.
struct Working {
    activity: ai::Activity,
    started: Instant,
    frame: usize,
    cancel: Arc<AtomicBool>,
}

/// What the background thread reports.
enum Report {
    Activity(ai::Activity),
    Done(Result<catalog::Task, String>),
}

/// What the user picked on the start screen.
pub enum Choice {
    Open(Course),
    Quit,
}

/// What the line above the key bar is asking for.
enum Asking {
    Topic,
    ApiKey,
    ImportPath,
    ExportPath { id: String },
}

struct Input {
    asking: Asking,
    text: String,
}

pub struct Menu {
    pub lang: Lang,
    pub root: PathBuf,
    courses: Vec<Course>,
    /// A course, or one of the two rows after them: AI and import.
    selected: usize,
    /// Shown above the key bar, e.g. when a directory can't be created.
    pub status: String,
    input: Option<Input>,
    working: Option<Working>,
}

impl Menu {
    pub fn new(root: PathBuf, lang: Lang) -> Self {
        Self {
            courses: lesson::courses(lang),
            lang,
            root,
            selected: 0,
            status: String::new(),
            input: None,
            working: None,
        }
    }

    fn ai_row(&self) -> usize {
        self.courses.len()
    }

    fn import_row(&self) -> usize {
        self.courses.len() + 1
    }

    /// Prints the CB-BIOS lines one by one. Any key skips the rest.
    pub fn boot(&self, out: &mut impl Write) -> io::Result<()> {
        let (w, h) = terminal::size()?;
        let (w, h) = (w as usize, h as usize);
        queue!(out, Hide)?;
        for y in 0..h {
            queue!(out, MoveTo(0, y as u16))?;
            put(out, "", w, theme::STATUS)?;
        }
        let dim = Paint::new(theme::FG_DIM, theme::BG);
        for (y, line) in boot_lines(self.lang).iter().enumerate().take(h) {
            queue!(out, MoveTo(1, y as u16 + 1))?;
            put(out, line, w.saturating_sub(1), dim)?;
            out.flush()?;
            if event::poll(BOOT_STEP)? {
                event::read()?;
                break;
            }
        }
        Ok(())
    }

    /// Shows the courses until one is picked or the user leaves. Creating
    /// a course with AI, importing and exporting happen right here.
    pub fn pick(&mut self, out: &mut impl Write) -> io::Result<Choice> {
        loop {
            // Draw once a burst of keys (a pasted path) is through.
            if !event::poll(Duration::ZERO)? {
                self.draw(out)?;
            }
            let event = event::read()?;
            if self.input.is_some() {
                self.type_input(event, out)?;
                continue;
            }
            let Event::Key(key) = event else {
                continue;
            };
            if key.kind == KeyEventKind::Release {
                continue;
            }
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let last = self.import_row();
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => self.selected = (self.selected + 1).min(last),
                KeyCode::Home => self.selected = 0,
                KeyCode::End => self.selected = last,
                KeyCode::Char(c @ '1'..='9') => {
                    let i = c as usize - '1' as usize;
                    if i < self.courses.len() {
                        self.selected = i;
                        return Ok(self.open());
                    }
                }
                KeyCode::Enter if self.selected == self.ai_row() => self.ask_topic(),
                KeyCode::Char('m' | 'M') if self.selected == self.ai_row() => self.switch_model(),
                KeyCode::Char('s' | 'S') if self.selected == self.ai_row() && !ctrl => {
                    self.ask(Asking::ApiKey, String::new());
                }
                KeyCode::Enter if self.selected == self.import_row() => {
                    self.ask(Asking::ImportPath, String::new());
                }
                KeyCode::Enter => return Ok(self.open()),
                KeyCode::Char('e' | 'E') if self.selected < self.courses.len() => {
                    let id = self.courses[self.selected].id.clone();
                    let path = format!("{id}.{}", catalog::EXTENSION);
                    self.ask(Asking::ExportPath { id }, path);
                }
                KeyCode::Char('l' | 'L') => {
                    self.lang = match self.lang {
                        Lang::De => Lang::En,
                        Lang::En => Lang::De,
                    };
                    self.courses = lesson::courses(self.lang);
                    self.status.clear();
                }
                KeyCode::Esc | KeyCode::Char('q') => return Ok(Choice::Quit),
                KeyCode::Char('c' | 'x') if ctrl => return Ok(Choice::Quit),
                _ => {}
            }
        }
    }

    fn open(&self) -> Choice {
        Choice::Open(lesson::courses(self.lang).swap_remove(self.selected))
    }

    fn ask(&mut self, asking: Asking, text: String) {
        self.input = Some(Input { asking, text });
        self.status.clear();
    }

    /// M on the AI row: the next model of the provider, saved right away.
    fn switch_model(&mut self) {
        let lang = self.lang;
        let mut config = ai::Config::load(&self.root);
        self.status = match config.next_model() {
            Ok(()) => tr!(
                lang,
                "[ Model: {} ]",
                "[ Modell: {} ]",
                config.model_name().unwrap_or_default()
            ),
            Err(e) => format!("[ {e} ]"),
        };
    }

    /// S on the AI row: stores the key that was typed or pasted.
    fn save_key(&mut self, key: &str) {
        let lang = self.lang;
        let mut config = ai::Config::load(&self.root);
        self.status = match config.save_key(key) {
            Ok(()) if ai::Config::key_from_env() => lang
                .pick(
                    "[ Key saved, but CB_AI_KEY / KICONNECT_API_KEY is set and wins ]",
                    "[ Schlüssel gespeichert, aber CB_AI_KEY / KICONNECT_API_KEY ist gesetzt und gilt vorher ]",
                )
                .into(),
            Ok(()) => tr!(
                lang,
                "[ Key saved ({} characters) ]",
                "[ Schlüssel gespeichert ({} Zeichen) ]",
                key.trim().chars().count()
            ),
            Err(e) => format!("[ {e} ]"),
        };
    }

    fn ask_topic(&mut self) {
        let config = ai::Config::load(&self.root);
        match config.missing(self.lang) {
            Some(missing) => self.status = missing,
            None => self.ask(Asking::Topic, String::new()),
        }
    }

    /// A key while the input line is open: edit it, run it with Enter,
    /// close it with Esc.
    fn type_input(&mut self, event: Event, out: &mut impl Write) -> io::Result<()> {
        let input = self.input.as_mut().expect("is open");
        match event {
            Event::Paste(text) => input.text.push_str(text.trim_end_matches(['\r', '\n'])),
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        || key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    input.text.push(c);
                }
                KeyCode::Backspace => {
                    input.text.pop();
                }
                KeyCode::Esc => {
                    self.input = None;
                    self.status = self.lang.pick("[ Cancelled ]", "[ Abgebrochen ]").into();
                }
                KeyCode::Enter => {
                    let input = self.input.take().expect("is open");
                    let text = input.text.trim().to_owned();
                    if !text.is_empty() {
                        match input.asking {
                            Asking::Topic => self.generate(&text, out)?,
                            Asking::ApiKey => self.save_key(&text),
                            Asking::ImportPath => self.import(&text),
                            Asking::ExportPath { id } => self.export(&id, &text),
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    /// Lets the AI write a course about `topic`, saves it and selects it.
    /// The request runs in the background, so the spinner keeps turning
    /// and pieces of the answer show up while they stream in; Esc cancels.
    fn generate(&mut self, topic: &str, out: &mut impl Write) -> io::Result<()> {
        let lang = self.lang;
        let config = ai::Config::load(&self.root);
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        {
            let topic = topic.to_owned();
            let cancel = Arc::clone(&cancel);
            thread::spawn(move || {
                let progress = tx.clone();
                let result = ai::generate(&config, &topic, lang, &cancel, &mut |a| {
                    let _ = progress.send(Report::Activity(a.clone()));
                });
                let _ = tx.send(Report::Done(result));
            });
        }
        self.working = Some(Working {
            activity: ai::Activity::default(),
            started: Instant::now(),
            frame: 0,
            cancel,
        });
        let result = loop {
            let report = rx.recv_timeout(SPIN_STEP);
            let working = self.working.as_mut().expect("set above");
            match report {
                Ok(Report::Activity(activity)) => working.activity = activity,
                Ok(Report::Done(result)) => break result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err(lang
                        .pick("the AI stopped", "die KI ist ausgestiegen")
                        .into());
                }
            }
            working.frame = working.started.elapsed().as_millis() as usize / 100;
            let mut cancelled = false;
            while event::poll(Duration::ZERO)? {
                if let Event::Key(key) = event::read()?
                    && key.kind != KeyEventKind::Release
                    && key.code == KeyCode::Esc
                {
                    cancelled = true;
                }
            }
            if cancelled {
                // Don't wait for the model: the thread stops at its next
                // piece, and whatever it still sends goes nowhere.
                working.cancel.store(true, Ordering::Relaxed);
                break Err(lang.pick("cancelled", "abgebrochen").into());
            }
            self.draw(out)?;
        };
        self.working = None;
        self.status = match result.and_then(|task| {
            catalog::save_custom(&task, lang)?;
            Ok(task)
        }) {
            Ok(task) => {
                self.select(&task.id);
                tr!(
                    lang,
                    "[ Course \"{}\" created. Enter opens it. ]",
                    "[ Kurs „{}“ erstellt. Enter öffnet ihn. ]",
                    task.title.get(lang)
                )
            }
            Err(e) => format!("[ {} ]", first_line(&e)),
        };
        Ok(())
    }

    fn import(&mut self, path: &str) {
        let lang = self.lang;
        self.status = match catalog::import(&expand_home(path), lang) {
            Ok((task, replaced)) => {
                self.select(&task.id);
                let title = task.title.get(lang);
                if replaced {
                    tr!(
                        lang,
                        "[ \"{title}\" updated ]",
                        "[ „{title}“ aktualisiert ]"
                    )
                } else {
                    tr!(lang, "[ \"{title}\" imported ]", "[ „{title}“ importiert ]")
                }
            }
            Err(e) => format!("[ {} ]", first_line(&e)),
        };
    }

    fn export(&mut self, id: &str, path: &str) {
        let lang = self.lang;
        let path = expand_home(path);
        self.status = match catalog::export(id, &path) {
            Ok(()) => {
                let shown = workspace::pretty(&std::path::absolute(&path).unwrap_or(path));
                tr!(
                    lang,
                    "[ Exported to {shown} ]",
                    "[ Exportiert nach {shown} ]"
                )
            }
            Err(e) => format!("[ {} ]", first_line(&e)),
        };
    }

    /// Reloads the courses and selects course `id`.
    fn select(&mut self, id: &str) {
        self.courses = lesson::courses(self.lang);
        if let Some(i) = self.courses.iter().position(|c| c.id == id) {
            self.selected = i;
        }
    }

    fn draw(&self, out: &mut impl Write) -> io::Result<()> {
        let (w, h) = terminal::size()?;
        let (w, h) = (w as usize, (h as usize).max(4));
        let lines = self.content(w.saturating_sub(4).clamp(20, 76));
        let margin = w.saturating_sub(76) / 2 + 2;
        queue!(out, Hide, MoveTo(0, 0))?;
        let brand = " ▓▒░ CBEditor";
        // A long directory keeps its end, which names it best.
        let room = w.saturating_sub(brand.chars().count() + 3);
        let root: Vec<char> = format!("{} ", workspace::pretty(&self.root))
            .chars()
            .collect();
        let root: String = if root.len() > room {
            let tail = &root[root.len() - room.saturating_sub(1)..];
            std::iter::once('…').chain(tail.iter().copied()).collect()
        } else {
            root.into_iter().collect()
        };
        let gap = w.saturating_sub(brand.chars().count() + root.chars().count());
        put_line(
            out,
            &[
                (brand.into(), theme::BAR.bold()),
                (" ".repeat(gap), theme::BAR),
                (root, theme::BAR),
            ],
            w,
            theme::BAR,
        )?;
        for y in 1..h - 2 {
            queue!(out, MoveTo(0, y as u16))?;
            let mut spans: Vec<Span> = vec![(" ".repeat(margin), theme::STATUS)];
            if let Some(line) = lines.get(y - 1) {
                spans.extend(line.iter().cloned());
            }
            put_line(out, &spans, w, theme::STATUS)?;
        }
        queue!(out, MoveTo(0, (h - 2) as u16))?;
        match &self.input {
            Some(input) => {
                let prompt = self.prompt(&input.asking);
                // The end of the text stays visible while typing.
                let room = w.saturating_sub(prompt.chars().count() + 3);
                let text: Vec<char> = match input.asking {
                    // The key isn't shown, only how long it is.
                    Asking::ApiKey => input.text.chars().map(|_| '•').collect(),
                    _ => input.text.chars().collect(),
                };
                let text: String = text[text.len().saturating_sub(room)..].iter().collect();
                put(out, &format!(" {prompt}{text}█"), w, theme::ASK)?;
            }
            None => match &self.working {
                Some(working) => {
                    let spin = SPINNER[working.frame % SPINNER.len()];
                    let text = format!("{spin} {}", working.activity.step);
                    put(out, &center(&text, w), w, theme::STATUS)?
                }
                None => put(out, &center(&self.status, w), w, theme::STATUS)?,
            },
        }
        self.draw_key_bar(out, w, (h - 1) as u16)?;
        out.flush()
    }

    /// Everything between the bars, `width` columns wide.
    fn content(&self, width: usize) -> Vec<Line> {
        let lang = self.lang;
        let mut out: Vec<Line> = vec![Vec::new()];
        let bright = Paint::new(theme::FG, theme::BG).bold();
        if width >= 52 {
            out.extend(BANNER.iter().map(|l| vec![(l.to_string(), bright)]));
        } else {
            out.push(vec![("CodeBetter".into(), bright)]);
        }
        out.push(Vec::new());
        let tagline = lang.pick(
            "        Learn to code. Keep the skill. Despite AI.",
            "        Coden lernen und behalten. Trotz KI.",
        );
        out.push(vec![(tagline.into(), Paint::new(theme::FG_DIM, theme::BG))]);
        out.push(Vec::new());

        let progress: Vec<Option<usize>> = self
            .courses
            .iter()
            .map(|c| workspace::completed(&self.root.join(&c.id), &c.id))
            .collect();
        let done = self
            .courses
            .iter()
            .zip(&progress)
            .filter(|(c, p)| p.is_some_and(|p| p >= c.lessons))
            .count();
        let total = self.courses.len();
        out.push(vec![(
            tr!(
                lang,
                "Welcome. {done} of {total} courses completed.",
                "Willkommen. {done} von {total} Kursen geschafft."
            ),
            theme::STATUS,
        )]);
        out.push(Vec::new());

        let head = format!(
            "══ {} ",
            lang.pick("LANGUAGE & PROJECT", "SPRACHE & PROJEKT")
        );
        let fill = "═".repeat(width.saturating_sub(head.chars().count()));
        out.push(vec![(head + &fill, Paint::new(theme::YELLOW, theme::BG))]);
        out.push(Vec::new());

        let title_w = width.saturating_sub(4 + 9 + 20).clamp(10, 40);
        for (i, (course, completed)) in self.courses.iter().zip(&progress).enumerate() {
            let selected = i == self.selected;
            let state = match completed {
                None => lang.pick("new", "neu").to_owned(),
                Some(n) if *n >= course.lessons => lang.pick("★ done", "★ geschafft").to_owned(),
                Some(n) => format!("{n}/{}", course.lessons),
            };
            let bar = check::progress_bar(completed.unwrap_or(0), course.lessons, 10);
            let own = if course.custom {
                lang.pick(" (own)", " (eigene)")
            } else {
                ""
            };
            let title: String = format!("{}{own}", course.title)
                .chars()
                .take(title_w)
                .collect();
            let text = format!(
                "{} {}  {:<8} {title:<title_w$}  {bar} {state}",
                if selected { "▶" } else { " " },
                i + 1,
                course.language(),
            );
            let paint = if selected {
                theme::FLASH.bold()
            } else {
                theme::STATUS
            };
            out.push(vec![(text, paint)]);
        }
        out.push(Vec::new());

        let actions = [
            (
                "+",
                lang.pick(
                    "Create a new course with AI",
                    "Neue Aufgabe mit KI erstellen",
                ),
            ),
            (
                "⇩",
                lang.pick("Import a course (.cbtask)", "Aufgabe importieren (.cbtask)"),
            ),
        ];
        let config = ai::Config::load(&self.root);
        for (i, (mark, label)) in actions.into_iter().enumerate() {
            let selected = self.selected == self.courses.len() + i;
            let text = format!("{} {mark}  {label}", if selected { "▶" } else { " " });
            let (paint, side) = if selected {
                (theme::FLASH.bold(), theme::FLASH)
            } else {
                (theme::STATUS, Paint::new(theme::FG_DIM, theme::BG))
            };
            let mut line = vec![(text, paint)];
            if i == 0 {
                line.push((format!("   {}", config.name), side));
            }
            out.push(line);
        }
        out.push(Vec::new());

        let lesson_paint = Paint::new(theme::LESSON, theme::BG);
        let note = Paint::new(theme::FG_DIM, theme::BG);
        if let Some(working) = &self.working {
            self.working_into(&mut out, working, width);
        } else if self.selected == self.ai_row() {
            let name = &config.name;
            let text = tr!(
                lang,
                "Name a topic, like \"Python: lists and loops\" or \"C: pointers\". \
                 {name} writes a course from it, with lessons, hints and tests. Every \
                 lesson is checked with a reference solution before the course is kept.",
                "Nenne ein Thema, z. B. „Python: Listen und Schleifen“ oder „C: Zeiger“. \
                 {name} schreibt daraus einen Kurs mit Lektionen, Tipps und Tests. Jede \
                 Lektion wird vor dem Speichern mit einer Musterlösung geprüft."
            );
            lesson::wrap_into(&mut out, &text, width, lesson_paint);
            out.push(Vec::new());
            let model = config
                .model_name()
                .unwrap_or_else(|| lang.pick("the API's first", "das erste der API").into());
            let mut line = vec![
                (lang.pick("Model:  ", "Modell:  ").into(), note),
                (model, Paint::new(theme::BLUE, theme::BG)),
            ];
            if config.can_switch_model() {
                line.push((lang.pick("   M switches", "   M wechselt").into(), note));
            }
            line.push((
                lang.pick("   S stores a key", "   S legt Schlüssel ab")
                    .into(),
                note,
            ));
            out.push(line);
            if let Some(missing) = config.missing(lang) {
                out.push(Vec::new());
                lesson::wrap_into(&mut out, &missing, width, Paint::new(theme::RED, theme::BG));
            }
        } else if self.selected == self.import_row() {
            let own = workspace::pretty(&self.root.join(".custom"));
            let text = tr!(
                lang,
                "Loads a course from a .cbtask file, e.g. from a teacher or other \
                 learners. E on a course exports it the same way. Own courses are kept in {own}.",
                "Lädt einen Kurs aus einer .cbtask-Datei, z. B. von Lehrenden oder anderen \
                 Lernenden. E auf einem Kurs exportiert ihn genauso. Eigene Kurse liegen in {own}."
            );
            lesson::wrap_into(&mut out, &text, width, lesson_paint);
        } else if let Some(course) = self.courses.get(self.selected) {
            lesson::wrap_into(&mut out, &course.summary, width, lesson_paint);
            out.push(Vec::new());
            let file = self.root.join(&course.id).join(&course.starter_file);
            let exists = file.exists();
            let label = match (exists, lang) {
                (true, Lang::De) => "Ordner:  ",
                (true, Lang::En) => "Folder:  ",
                (false, Lang::De) => "Wird angelegt:  ",
                (false, Lang::En) => "Will be created:  ",
            };
            out.push(vec![
                (label.into(), note),
                (workspace::pretty(&file), Paint::new(theme::BLUE, theme::BG)),
            ]);
        }
        out
    }

    fn draw_key_bar(&self, out: &mut impl Write, w: usize, y: u16) -> io::Result<()> {
        let lang = self.lang;
        let keys = [
            ("↑↓", lang.pick("Select", "Auswählen")),
            ("Enter", lang.pick("Open", "Öffnen")),
            ("E", lang.pick("Export", "Exportieren")),
            ("L", lang.pick("Deutsch", "English")),
            ("Esc", lang.pick("Quit", "Beenden")),
        ];
        let mut spans: Vec<Span> = vec![(" ".into(), theme::BAR)];
        for (key, label) in keys {
            spans.push((format!(" {key} "), theme::KEY));
            spans.push((format!(" {label}  "), theme::BAR));
        }
        queue!(out, MoveTo(0, y))?;
        put_line(out, &spans, w, theme::BAR)
    }
}

impl Menu {
    /// The AI pane while a course is being written:
    ///
    /// ```text
    /// ══ KI ARBEITET ═══════════════════════════════
    /// ⠹ KI:connect.nrw (glm-4.7_stud) schreibt den Kurs
    ///   0:42 · 3.1k Zeichen · Lektion 2: Schleifen
    ///   › for i in range(1, n + 1):
    ///   ›     print(i)
    ///   Esc bricht ab
    /// ```
    fn working_into(&self, out: &mut Vec<Line>, working: &Working, width: usize) {
        let lang = self.lang;
        let a = &working.activity;
        let head = format!("══ {} ", lang.pick("AI AT WORK", "KI ARBEITET"));
        let fill = "═".repeat(width.saturating_sub(head.chars().count()));
        out.push(vec![(head + &fill, Paint::new(theme::YELLOW, theme::BG))]);
        let spin = SPINNER[working.frame % SPINNER.len()];
        let step = if a.step.is_empty() {
            lang.pick("starting ...", "starte ...").to_owned()
        } else {
            a.step.clone()
        };
        out.push(vec![
            (
                format!("{spin} "),
                Paint::new(theme::YELLOW, theme::BG).bold(),
            ),
            (step, theme::STATUS),
        ]);

        let secs = working.started.elapsed().as_secs();
        let mut facts = vec![format!("{}:{:02}", secs / 60, secs % 60)];
        if a.chars > 0 {
            let chars = if a.chars >= 1000 {
                format!("{:.1}k", a.chars as f64 / 1000.0)
            } else {
                a.chars.to_string()
            };
            facts.push(tr!(lang, "{chars} characters", "{chars} Zeichen"));
        }
        if a.thinking {
            facts.push(lang.pick("thinking ...", "denkt nach ...").into());
        } else if let Some(lesson) = &a.lesson {
            facts.push(lesson.clone());
        }
        let note = Paint::new(theme::FG_DIM, theme::BG);
        out.push(vec![(format!("  {}", facts.join(" · ")), note)]);

        // Only a glimpse of the answer, cut to one line each.
        let room = width.saturating_sub(4).max(8);
        let preview = Paint::new(theme::LESSON, theme::BG);
        for line in &a.preview {
            let chars: Vec<char> = line.chars().collect();
            let shown: String = if chars.len() > room {
                std::iter::once('…')
                    .chain(chars[chars.len() - (room - 1)..].iter().copied())
                    .collect()
            } else {
                line.clone()
            };
            let paint = if a.thinking { note } else { preview };
            out.push(vec![("  › ".into(), note), (shown, paint)]);
        }
        out.push(Vec::new());
        out.push(vec![(
            lang.pick("  Esc cancels", "  Esc bricht ab").into(),
            note,
        )]);
    }

    fn prompt(&self, asking: &Asking) -> &'static str {
        let lang = self.lang;
        match asking {
            Asking::Topic => lang.pick(
                "Topic (e.g. Python: lists and loops): ",
                "Thema (z. B. Python: Listen und Schleifen): ",
            ),
            Asking::ImportPath => {
                lang.pick("Import file (.cbtask): ", "Datei importieren (.cbtask): ")
            }
            Asking::ApiKey => lang.pick("API key (paste it): ", "API-Schlüssel (einfügen): "),
            Asking::ExportPath { .. } => lang.pick("Export to: ", "Exportieren nach: "),
        }
    }
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or_default()
}

/// `~/x` as a path in the home directory.
fn expand_home(path: &str) -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    match (path.strip_prefix("~/").or(path.strip_prefix("~\\")), home) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest),
        _ => PathBuf::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: Vec<Line>) -> Vec<String> {
        lines
            .into_iter()
            .map(|spans| spans.into_iter().map(|(s, _)| s).collect())
            .collect()
    }

    #[test]
    fn lists_every_course_with_its_language() {
        let root = std::env::temp_dir().join("cbeditor-menu-empty");
        let mut menu = Menu::new(root, Lang::De);
        let text = texts(menu.content(76)).join("\n");
        for lang in ["Rust", "Java", "Python", "C "] {
            assert!(text.contains(lang), "{lang} missing:\n{text}");
        }
        assert!(text.contains("▶ 1  Rust"), "{text}");
        assert!(text.contains("Wird angelegt:"), "{text}");
        menu.selected = 2;
        let text = texts(menu.content(76)).join("\n");
        assert!(text.contains("▶ 3  Python"), "{text}");
        assert!(text.contains("03_python_basics/main.py"), "{text}");
        for line in texts(menu.content(40)) {
            assert!(line.chars().count() <= 76, "too wide: {line:?}");
        }
    }
}
