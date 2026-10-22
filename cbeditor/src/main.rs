//! CBEditor: the CodeBetter code editor for the terminal, built on crossterm
//! so it runs natively on Linux, macOS and Windows. Looks and behaves like
//! the CBEditor in the CodeBetter terminal app: code on the left, the lesson
//! on the right.
//!
//! Usage: cbeditor <file> [--task ID] [--part N] [--lang de|en]
//! Keys: ^S save, ^X exit, F5 check, F2 next hint, F1 toggle the panel,
//! Tab/Shift+Tab indent. The mouse wheel scrolls whatever it is over; the
//! panel also scrolls with Alt/Ctrl+Up/Down and Alt+PgUp/PgDn.

mod check;
mod draw;
mod i18n;
mod lesson;
mod runner;
mod theme;

use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use std::{env, fs};

use crossterm::cursor::{SetCursorStyle, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::style::ResetColor;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};

use i18n::Lang;
use lesson::Lesson;

const INDENT: &str = "    ";
/// Lines per mouse wheel step.
const WHEEL_LINES: usize = 3;
/// How long a fresh hint or lesson flashes (4 blinks, like the app).
const FLASH: Duration = Duration::from_millis(1000);

/// What the main loop should do after a key.
enum Action {
    Continue,
    Check,
    Quit,
}

struct Editor {
    path: String,
    /// Shown in the top bar.
    name: String,
    lang: Lang,
    /// Lines without their trailing newline.
    lines: Vec<String>,
    /// Cursor position: row in `lines`, column in chars (not bytes).
    row: usize,
    col: usize,
    /// First visible row and column.
    top: usize,
    left: usize,
    modified: bool,
    /// Asking whether to save before exiting.
    asking: bool,
    status: String,
    lesson: Option<Lesson>,
    show_panel: bool,
    /// First visible line of the panel.
    panel_top: usize,
    /// Screen column where the panel starts, `None` while it's hidden.
    panel_x: Option<usize>,
    /// When the newest hint or lesson stops flashing.
    flash_until: Option<Instant>,
}

impl Editor {
    fn open(path: String, lesson: Option<Lesson>, lang: Lang) -> Self {
        let (lines, status) = match fs::read_to_string(&path) {
            Ok(text) => {
                let lines: Vec<String> = text.lines().map(str::to_owned).collect();
                let n = lines.len();
                (
                    lines,
                    tr!(lang, "[ Read {n} lines ]", "[ {n} Zeilen gelesen ]"),
                )
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (
                Vec::new(),
                lang.pick("[ New File ]", "[ Neue Datei ]").into(),
            ),
            Err(e) => (
                Vec::new(),
                tr!(lang, "[ Error reading: {e} ]", "[ Fehler beim Lesen: {e} ]"),
            ),
        };
        let name = Path::new(&path)
            .file_name()
            .map_or(path.clone(), |n| n.to_string_lossy().into_owned());
        Self {
            path,
            name,
            lang,
            lines: if lines.is_empty() {
                vec![String::new()]
            } else {
                lines
            },
            row: 0,
            col: 0,
            top: 0,
            left: 0,
            modified: false,
            asking: false,
            status,
            lesson,
            show_panel: true,
            panel_top: 0,
            panel_x: None,
            flash_until: None,
        }
    }

    fn save(&mut self) -> bool {
        let mut text = self.lines.join("\n");
        text.push('\n');
        let lang = self.lang;
        match fs::write(&self.path, text) {
            Ok(()) => {
                self.modified = false;
                let n = self.lines.len();
                self.status = tr!(lang, "[ Wrote {n} lines ]", "[ {n} Zeilen geschrieben ]");
                true
            }
            Err(e) => {
                self.status = tr!(
                    lang,
                    "[ Error writing: {e} ]",
                    "[ Fehler beim Schreiben: {e} ]"
                );
                false
            }
        }
    }

    fn line_len(&self, row: usize) -> usize {
        self.lines[row].chars().count()
    }

    /// Byte index of char column `col` in the current line.
    fn byte_at(&self, col: usize) -> usize {
        let line = &self.lines[self.row];
        line.char_indices().nth(col).map_or(line.len(), |(i, _)| i)
    }

    fn insert(&mut self, s: &str) {
        let at = self.byte_at(self.col);
        self.lines[self.row].insert_str(at, s);
        self.col += s.chars().count();
        self.modified = true;
    }

    /// Splits the line, keeping its indentation, plus one level after an
    /// opening `{`, `(` or `[`.
    fn newline(&mut self) {
        let at = self.byte_at(self.col);
        let line = &self.lines[self.row];
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let opens = line[..at].trim_end().ends_with(['{', '(', '[']);
        let rest = self.lines[self.row].split_off(at);
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.col = 0;
        self.insert(&indent);
        if opens {
            self.insert(INDENT);
        }
        self.modified = true;
    }

    /// Removes up to one level of leading spaces from the current line.
    fn outdent(&mut self) {
        let line = &mut self.lines[self.row];
        let n = line
            .chars()
            .take(INDENT.len())
            .take_while(|c| *c == ' ')
            .count();
        if n > 0 {
            line.drain(..n);
            self.col = self.col.saturating_sub(n);
            self.modified = true;
        }
    }

    fn backspace(&mut self) {
        if self.col > 0 {
            self.col -= 1;
            let at = self.byte_at(self.col);
            self.lines[self.row].remove(at);
        } else if self.row > 0 {
            let line = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.line_len(self.row);
            self.lines[self.row].push_str(&line);
        } else {
            return;
        }
        self.modified = true;
    }

    fn delete(&mut self) {
        if self.col < self.line_len(self.row) {
            let at = self.byte_at(self.col);
            self.lines[self.row].remove(at);
        } else if self.row + 1 < self.lines.len() {
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
        } else {
            return;
        }
        self.modified = true;
    }

    /// Moves up/down by `delta` rows, keeping the column inside the line.
    fn move_rows(&mut self, delta: isize) {
        let last = self.lines.len() - 1;
        self.row = self.row.saturating_add_signed(delta).min(last);
        self.col = self.col.min(self.line_len(self.row));
    }

    fn hint(&mut self) {
        if let Some(lesson) = &mut self.lesson
            && lesson.reveal_hint()
        {
            // Scroll to the end so the new hint is visible.
            self.show_panel = true;
            self.panel_top = usize::MAX;
            self.flash_until = Some(Instant::now() + FLASH);
        }
    }

    /// F5: saves, then runs the lesson's tests. The panel shows "checking"
    /// while `draw` runs, then the result; a pass shows the next lesson
    /// from the top.
    fn check(&mut self, draw: &mut impl FnMut(&mut Self) -> io::Result<()>) -> io::Result<()> {
        if self.lesson.is_none() || !self.save() {
            return Ok(());
        }
        let lesson = self.lesson.as_mut().expect("checked above");
        lesson.start_check();
        self.show_panel = true;
        self.panel_top = usize::MAX;
        draw(self)?;
        let lesson = self.lesson.as_mut().expect("checked above");
        if lesson.check(Path::new(&self.path)) {
            self.panel_top = 0;
            self.flash_until = Some(Instant::now() + FLASH);
        } else {
            self.panel_top = usize::MAX;
        }
        Ok(())
    }

    /// Answers "save modified buffer?".
    fn answer(&mut self, key: KeyEvent) -> Action {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('y' | 'Y' | 'j' | 'J') if self.save() => return Action::Quit,
            KeyCode::Char('y' | 'Y' | 'j' | 'J') => return Action::Continue,
            KeyCode::Char('n' | 'N') => return Action::Quit,
            KeyCode::Esc => {}
            KeyCode::Char('c') if ctrl => {}
            _ => return Action::Continue,
        }
        self.asking = false;
        self.status = self.lang.pick("[ Cancelled ]", "[ Abgebrochen ]").into();
        Action::Continue
    }

    /// Scrolls the panel by `delta` lines; `draw` keeps it in range.
    fn scroll_panel(&mut self, delta: isize) {
        self.panel_top = self.panel_top.saturating_add_signed(delta);
    }

    /// The wheel scrolls the panel when over it, otherwise the code.
    fn mouse(&mut self, mouse: MouseEvent) {
        let delta = match mouse.kind {
            MouseEventKind::ScrollUp => -(WHEEL_LINES as isize),
            MouseEventKind::ScrollDown => WHEEL_LINES as isize,
            _ => return,
        };
        match self.panel_x {
            Some(x) if mouse.column as usize >= x => self.scroll_panel(delta),
            _ => self.move_rows(delta),
        }
    }

    /// Handles one key press.
    fn handle(&mut self, key: KeyEvent, page: usize) -> Action {
        if self.asking {
            return self.answer(key);
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char('s') if ctrl => {
                self.save();
            }
            KeyCode::Char('x' | 'q') if ctrl => {
                if !self.modified {
                    return Action::Quit;
                }
                self.asking = true;
                self.status = self
                    .lang
                    .pick(
                        "Save modified buffer?  Y = yes   N = no   Esc = cancel",
                        "Geänderten Puffer speichern?  J = ja   N = nein   Esc = abbrechen",
                    )
                    .into();
            }
            KeyCode::F(5) => return Action::Check,
            KeyCode::F(1) => self.show_panel = !self.show_panel,
            KeyCode::F(2) => self.hint(),
            // Many terminals keep Alt+arrows for themselves, so Ctrl works too.
            KeyCode::Up if alt || ctrl => self.scroll_panel(-1),
            KeyCode::Down if alt || ctrl => self.scroll_panel(1),
            KeyCode::PageUp if alt => self.scroll_panel(-(page as isize)),
            KeyCode::PageDown if alt => self.scroll_panel(page as isize),
            KeyCode::Char(c) if !ctrl => self.insert(c.encode_utf8(&mut [0; 4])),
            KeyCode::Tab => self.insert(INDENT),
            KeyCode::BackTab => self.outdent(),
            KeyCode::Enter => self.newline(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left if self.col > 0 => self.col -= 1,
            KeyCode::Left if self.row > 0 => {
                self.row -= 1;
                self.col = self.line_len(self.row);
            }
            KeyCode::Right if self.col < self.line_len(self.row) => self.col += 1,
            KeyCode::Right if self.row + 1 < self.lines.len() => {
                self.row += 1;
                self.col = 0;
            }
            KeyCode::Up => self.move_rows(-1),
            KeyCode::Down => self.move_rows(1),
            KeyCode::PageUp => self.move_rows(-(page as isize)),
            KeyCode::PageDown => self.move_rows(page as isize),
            KeyCode::Home => self.col = 0,
            KeyCode::End => self.col = self.line_len(self.row),
            _ => {}
        }
        Action::Continue
    }
}

fn run(editor: &mut Editor) -> io::Result<()> {
    let mut out = io::stdout();
    loop {
        editor.draw(&mut out)?;
        // While a hint flashes, wake up to redraw it; otherwise wait for input.
        if let Some(until) = editor.flash_until {
            if Instant::now() >= until {
                editor.flash_until = None;
                continue;
            }
            if !event::poll(draw::FLASH_STEP)? {
                continue;
            }
        }
        let page = draw::text_height(terminal::size()?.1 as usize);
        match event::read()? {
            // Windows also reports key releases; only react to presses.
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                match editor.handle(key, page) {
                    Action::Continue => {}
                    Action::Check => editor.check(&mut |e: &mut Editor| e.draw(&mut out))?,
                    Action::Quit => return Ok(()),
                }
            }
            Event::Mouse(mouse) => editor.mouse(mouse),
            _ => {}
        }
    }
}

/// Returns the file to edit, the lesson for the panel and the language.
/// Without `--task`, the panel shows the first course.
fn parse_args(
    mut args: impl Iterator<Item = String>,
) -> Result<(String, Option<Lesson>, Lang), String> {
    let mut path = None;
    let mut task = None;
    let mut part = 1;
    let mut lang = Lang::De;
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--task" => task = Some(value()?),
            "--part" => part = value()?.parse().map_err(|_| "--part needs a number")?,
            "--lang" => lang = Lang::parse(&value()?).ok_or("--lang is de or en")?,
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}")),
            _ if path.is_none() => path = Some(arg),
            _ => return Err("only one file at a time".into()),
        }
    }
    let path = path.ok_or("no file given")?;
    let lesson = Lesson::load(task.as_deref(), part, lang)?;
    Ok((path, Some(lesson), lang))
}

fn main() -> io::Result<()> {
    let (path, lesson, lang) = match parse_args(env::args().skip(1)) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("cbeditor: {e}");
            eprintln!("usage: cbeditor <file> [--task ID] [--part N] [--lang de|en]");
            std::process::exit(2);
        }
    };
    let mut editor = Editor::open(path, lesson, lang);

    terminal::enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        SetCursorStyle::BlinkingBlock
    )?;
    let result = run(&mut editor);
    // Always restore the terminal, even if drawing failed.
    let mut out = io::stdout();
    let _ = execute!(
        out,
        ResetColor,
        SetCursorStyle::DefaultUserShape,
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    );
    let _ = out.flush();
    let _ = terminal::disable_raw_mode();
    result
}
