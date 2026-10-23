//! CBEditor: the CodeBetter code editor for the terminal, built on crossterm
//! so it runs natively on Linux, macOS and Windows. Looks and behaves like
//! the CBEditor in the CodeBetter terminal app: code on the left, the lesson
//! on the right.
//!
//! Usage: cbeditor [file] [--task ID] [--part N] [--lang de|en] [--dir DIR]
//! Without a file it starts like the app: a short boot, the CodeBetter
//! banner and the courses. The picked course gets its own directory in
//! ~/Tasks.cb (or DIR), and the editor opens its starter file there.
//! Keys: ^S save, ^X exit, ^Z/^Y undo/redo, ^K/^U cut/paste a line, F5
//! check, F2 next hint, F1 toggle the panel, Tab/Shift+Tab indent. The mouse
//! wheel scrolls whatever it is over; the panel also scrolls with
//! Alt/Ctrl+Up/Down and Alt+PgUp/PgDn. Code is highlighted by file type.

mod ai;
mod catalog;
mod check;
mod draw;
mod highlight;
mod i18n;
mod lesson;
mod menu;
mod runner;
mod theme;
mod workspace;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{env, fs};

use crossterm::cursor::{SetCursorStyle, Show};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::style::ResetColor;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};

use highlight::Syntax;
use i18n::Lang;
use lesson::Lesson;
use menu::{Choice, Menu};

const INDENT: &str = "    ";
/// Lines per mouse wheel step.
const WHEEL_LINES: usize = 3;
/// How long a fresh hint or lesson flashes (4 blinks, like the app).
const FLASH: Duration = Duration::from_millis(1000);
/// Undo steps kept.
const UNDO_LIMIT: usize = 500;
/// Brackets and quotes that get closed when typed.
const PAIRS: [(char, char); 5] = [('(', ')'), ('[', ']'), ('{', '}'), ('"', '"'), ('\'', '\'')];

/// The file as it was before an edit, for undo and redo.
struct Snapshot {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

/// Kinds of edits. A run of typing (one word) or deleting is undone at once.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edit {
    Typing,
    Deleting,
    Other,
}

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
    syntax: &'static Syntax,
    /// Lines without their trailing newline.
    lines: Vec<String>,
    /// The lines as last read or written, to tell whether undo got back there.
    saved: Vec<String>,
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
    /// The course directory whose progress a pass moves on.
    progress_dir: Option<PathBuf>,
    /// First visible line of the panel.
    panel_top: usize,
    /// Screen column where the panel starts, `None` while it's hidden.
    panel_x: Option<usize>,
    /// When the newest hint or lesson stops flashing.
    flash_until: Option<Instant>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The edit of the current key, and of the key before it.
    last_edit: Option<Edit>,
    prev_edit: Option<Edit>,
    /// Lines cut with ^K; cutting again right away adds to them.
    clipboard: Vec<String>,
    cutting: bool,
    /// Keys are arriving faster than anyone types: text is being pasted, so
    /// it goes in as it is, without auto-indent or closing brackets.
    pasting: bool,
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
        let lines = if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        };
        Self {
            syntax: Syntax::for_path(&path),
            path,
            name,
            lang,
            saved: lines.clone(),
            lines,
            row: 0,
            col: 0,
            top: 0,
            left: 0,
            modified: false,
            asking: false,
            status,
            lesson,
            show_panel: true,
            progress_dir: None,
            panel_top: 0,
            panel_x: None,
            flash_until: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            prev_edit: None,
            clipboard: Vec::new(),
            cutting: false,
            pasting: false,
        }
    }

    fn save(&mut self) -> bool {
        let mut text = self.lines.join("\n");
        text.push('\n');
        let lang = self.lang;
        match fs::write(&self.path, text) {
            Ok(()) => {
                self.modified = false;
                self.saved = self.lines.clone();
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

    /// The char at column `col` of the current line.
    fn char_at(&self, col: usize) -> Option<char> {
        self.lines[self.row].chars().nth(col)
    }

    /// Whether only spaces come before the cursor.
    fn in_indent(&self) -> bool {
        self.lines[self.row]
            .chars()
            .take(self.col)
            .all(|c| c == ' ')
    }

    /// Remembers the file before an edit so ^Z can bring it back.
    fn checkpoint(&mut self, edit: Edit) {
        let same_run = edit != Edit::Other && self.prev_edit == Some(edit);
        self.last_edit = Some(edit);
        if same_run {
            return;
        }
        self.undo.push(self.snapshot());
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            row: self.row,
            col: self.col,
        }
    }

    /// ^Z with `undo`, ^Y without: goes back or forward one step.
    fn undo(&mut self, undo: bool) {
        let lang = self.lang;
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        // Skip steps that changed nothing, like Backspace at the very start.
        while let Some(step) = from.pop() {
            if step.lines == self.lines {
                continue;
            }
            to.push(Snapshot {
                lines: std::mem::replace(&mut self.lines, step.lines),
                row: self.row,
                col: self.col,
            });
            self.row = step.row.min(self.lines.len() - 1);
            self.col = step.col.min(self.line_len(self.row));
            self.modified = self.lines != self.saved;
            self.status = if undo {
                lang.pick("[ Undone ]", "[ Rückgängig gemacht ]")
            } else {
                lang.pick("[ Redone ]", "[ Wiederhergestellt ]")
            }
            .into();
            return;
        }
        self.status = if undo {
            lang.pick("[ Nothing to undo ]", "[ Nichts rückgängig zu machen ]")
        } else {
            lang.pick("[ Nothing to redo ]", "[ Nichts wiederherzustellen ]")
        }
        .into();
    }

    fn insert(&mut self, s: &str) {
        let at = self.byte_at(self.col);
        self.lines[self.row].insert_str(at, s);
        self.col += s.chars().count();
        self.modified = true;
    }

    /// Types one char. Brackets and quotes get their closing partner, typing
    /// the partner steps over it, and a closing bracket at the start of a
    /// line moves back one level.
    fn type_char(&mut self, c: char) {
        self.checkpoint(if c == ' ' { Edit::Other } else { Edit::Typing });
        if self.pasting {
            self.insert(c.encode_utf8(&mut [0; 4]));
            return;
        }
        let next = self.char_at(self.col);
        let prev = self.col.checked_sub(1).and_then(|i| self.char_at(i));
        let quote = c == '"' || (c == '\'' && self.syntax.close_single_quote);
        if next == Some(c) && (matches!(c, ')' | ']' | '}') || quote) {
            self.col += 1;
            return;
        }
        if matches!(c, ')' | ']' | '}') && self.in_indent() {
            self.outdent();
        }
        let close = PAIRS
            .iter()
            .find(|(open, _)| *open == c)
            .map(|(_, close)| *close);
        let room = next.is_none_or(|n| n.is_whitespace() || ")]},;:".contains(n));
        // A quote after a letter is an apostrophe ("don't"), not a string.
        let opens_string = prev.is_none_or(|p| !p.is_alphanumeric() && p != c);
        if let Some(close) = close
            && room
            && (!matches!(c, '"' | '\'') || (quote && opens_string))
        {
            self.insert(&format!("{c}{close}"));
            self.col -= 1;
            return;
        }
        self.insert(c.encode_utf8(&mut [0; 4]));
    }

    /// Splits the line, keeping its indentation, plus one level after an
    /// opening `{`, `(` or `[` (or `:` in Python). Between a pair like `{}`
    /// the closing bracket goes on its own line.
    fn newline(&mut self) {
        self.checkpoint(Edit::Other);
        let at = self.byte_at(self.col);
        let rest = self.lines[self.row].split_off(at);
        self.modified = true;
        if self.pasting {
            self.row += 1;
            self.col = 0;
            self.lines.insert(self.row, rest);
            return;
        }
        let line = &self.lines[self.row];
        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let before = line.trim_end();
        let opens = before.ends_with(['{', '(', '['])
            || (self.syntax.colon_blocks && before.ends_with(':'));
        let closes = opens && rest.trim_start().starts_with(['}', ')', ']']);
        let mut new_line = indent.clone();
        if opens {
            new_line.push_str(INDENT);
        }
        self.row += 1;
        self.col = new_line.chars().count();
        if closes {
            self.lines.insert(self.row, new_line);
            self.lines.insert(self.row + 1, indent + rest.trim_start());
        } else {
            self.lines.insert(self.row, new_line + &rest);
        }
    }

    /// Tab: spaces up to the next multiple of four.
    fn indent(&mut self) {
        self.checkpoint(Edit::Other);
        let n = INDENT.len() - self.col % INDENT.len();
        self.insert(&INDENT[..n]);
    }

    /// Removes up to one level of leading spaces from the current line.
    fn outdent(&mut self) {
        self.checkpoint(Edit::Other);
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

    /// Deletes the char before the cursor; an empty pair like `()` goes
    /// at once, and in the indentation a whole level.
    fn backspace(&mut self) {
        self.checkpoint(Edit::Deleting);
        if self.col > 0 {
            let prev = self.char_at(self.col - 1);
            let next = self.char_at(self.col);
            let pair = PAIRS
                .iter()
                .any(|&(o, c)| prev == Some(o) && next == Some(c));
            let n = if self.pasting {
                1
            } else if self.in_indent() {
                (self.col - 1) % INDENT.len() + 1
            } else {
                1
            };
            self.col -= n;
            let at = self.byte_at(self.col);
            let len = if pair && !self.pasting { 2 } else { n };
            let end = self.byte_at(self.col + len);
            self.lines[self.row].replace_range(at..end, "");
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
        self.checkpoint(Edit::Deleting);
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

    /// ^K: cuts the current line. Cutting again right after adds the next
    /// line, so several lines can be moved with ^K ^K ^K and one ^U.
    fn cut_line(&mut self, append: bool) {
        self.checkpoint(Edit::Other);
        if !append {
            self.clipboard.clear();
        }
        let line = if self.lines.len() > 1 {
            self.lines.remove(self.row)
        } else {
            std::mem::take(&mut self.lines[0])
        };
        self.clipboard.push(line);
        self.row = self.row.min(self.lines.len() - 1);
        self.col = 0;
        self.cutting = true;
        self.modified = true;
        let n = self.clipboard.len();
        self.status = tr!(
            self.lang,
            "[ Cut {n} line(s) ]",
            "[ {n} Zeile(n) ausgeschnitten ]"
        );
    }

    /// ^U: puts the cut lines above the cursor line.
    fn paste_lines(&mut self) {
        let lang = self.lang;
        if self.clipboard.is_empty() {
            self.status = lang
                .pick("[ Nothing cut yet ]", "[ Noch nichts ausgeschnitten ]")
                .into();
            return;
        }
        self.checkpoint(Edit::Other);
        let n = self.clipboard.len();
        self.lines
            .splice(self.row..self.row, self.clipboard.iter().cloned());
        self.row += n;
        self.col = 0;
        self.modified = true;
        self.status = tr!(lang, "[ Pasted {n} line(s) ]", "[ {n} Zeile(n) eingefügt ]");
    }

    /// Text pasted with bracketed paste: inserted as it is, in one undo step.
    fn paste(&mut self, text: &str) {
        self.prev_edit = self.last_edit.take();
        self.checkpoint(Edit::Other);
        let text = text
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\t', INDENT);
        let at = self.byte_at(self.col);
        let rest = self.lines[self.row].split_off(at);
        let mut parts = text.split('\n');
        self.lines[self.row].push_str(parts.next().unwrap_or(""));
        for part in parts {
            self.row += 1;
            self.lines.insert(self.row, part.to_owned());
        }
        self.col = self.line_len(self.row);
        self.lines[self.row].push_str(&rest);
        self.modified = true;
    }

    /// Moves up/down by `delta` rows, keeping the column inside the line.
    fn move_rows(&mut self, delta: isize) {
        let last = self.lines.len() - 1;
        self.row = self.row.saturating_add_signed(delta).min(last);
        self.col = self.col.min(self.line_len(self.row));
    }

    /// Home: to the first non-space char, or to column 0 if already there.
    fn home(&mut self) {
        let indent = self.lines[self.row]
            .chars()
            .take_while(|c| c.is_whitespace())
            .count();
        self.col = if self.col == indent { 0 } else { indent };
    }

    /// Ctrl+Left: to the start of the word before the cursor.
    fn word_left(&mut self) {
        if self.col == 0 {
            if self.row > 0 {
                self.row -= 1;
                self.col = self.line_len(self.row);
            }
            return;
        }
        let chars: Vec<char> = self.lines[self.row].chars().collect();
        let mut col = self.col;
        while col > 0 && !is_word(chars[col - 1]) {
            col -= 1;
        }
        while col > 0 && is_word(chars[col - 1]) {
            col -= 1;
        }
        self.col = col;
    }

    /// Ctrl+Right: to the end of the next word.
    fn word_right(&mut self) {
        let chars: Vec<char> = self.lines[self.row].chars().collect();
        if self.col >= chars.len() {
            if self.row + 1 < self.lines.len() {
                self.row += 1;
                self.col = 0;
            }
            return;
        }
        let mut col = self.col;
        while col < chars.len() && !is_word(chars[col]) {
            col += 1;
        }
        while col < chars.len() && is_word(chars[col]) {
            col += 1;
        }
        self.col = col;
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
            if let Some(dir) = &self.progress_dir
                && let Err(e) = workspace::save_progress(dir, lesson)
            {
                self.status = tr!(
                    self.lang,
                    "[ Progress not saved: {e} ]",
                    "[ Fortschritt nicht gespeichert: {e} ]"
                );
            }
            self.panel_top = 0;
            self.flash_until = Some(Instant::now() + FLASH);
        } else {
            self.panel_top = usize::MAX;
        }
        Ok(())
    }

    /// Answers "save modified buffer?".
    fn answer(&mut self, key: KeyEvent) -> Action {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL)
            && !key.modifiers.contains(KeyModifiers::ALT);
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
        self.last_edit = None;
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
        // Edits of one run (typing a word) are undone together, and ^K adds
        // to the clipboard, only while no other key comes in between.
        self.prev_edit = self.last_edit.take();
        let cutting = std::mem::take(&mut self.cutting);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        // Windows reports AltGr as Ctrl+Alt, and AltGr types text on many
        // layouts (German: { } [ ] \ | @ ~), so Ctrl+Alt is never a shortcut.
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL) && !alt;
        match key.code {
            KeyCode::Char('s') if ctrl => {
                self.save();
            }
            KeyCode::Char('z') if ctrl => self.undo(true),
            KeyCode::Char('y') if ctrl => self.undo(false),
            KeyCode::Char('k') if ctrl => self.cut_line(cutting),
            KeyCode::Char('u') if ctrl => self.paste_lines(),
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
            KeyCode::Char(c) if !ctrl => self.type_char(c),
            KeyCode::Tab => self.indent(),
            KeyCode::BackTab => self.outdent(),
            KeyCode::Enter => self.newline(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left if ctrl => self.word_left(),
            KeyCode::Right if ctrl => self.word_right(),
            KeyCode::Home if ctrl => (self.row, self.col) = (0, 0),
            KeyCode::End if ctrl => {
                self.row = self.lines.len() - 1;
                self.col = self.line_len(self.row);
            }
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
            KeyCode::Home => self.home(),
            KeyCode::End => self.col = self.line_len(self.row),
            _ => {}
        }
        Action::Continue
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn run(editor: &mut Editor) -> io::Result<()> {
    let mut out = io::stdout();
    let mut queued_before = false;
    loop {
        // Draw once a burst of pasted keys is through, not after every key.
        if !event::poll(Duration::ZERO)? {
            editor.draw(&mut out)?;
        }
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
        let event = event::read()?;
        // A key that arrives together with more keys was pasted, not typed
        // (Windows has no bracketed paste, so a paste comes in as keys).
        let queued = event::poll(Duration::ZERO)?;
        editor.pasting = queued || queued_before;
        queued_before = queued;
        match event {
            // Windows also reports key releases; only react to presses.
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                match editor.handle(key, page) {
                    Action::Continue => {}
                    Action::Check => editor.check(&mut |e: &mut Editor| e.draw(&mut out))?,
                    Action::Quit => return Ok(()),
                }
            }
            Event::Mouse(mouse) => editor.mouse(mouse),
            Event::Paste(text) => editor.paste(&text),
            _ => {}
        }
    }
}

/// What to start with.
struct Args {
    path: Option<String>,
    task: Option<String>,
    part: Option<usize>,
    lang: Lang,
    root: PathBuf,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut a = Args {
        path: None,
        task: None,
        part: None,
        lang: Lang::De,
        root: workspace::default_root(),
    };
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--task" => a.task = Some(value()?),
            "--part" => a.part = Some(value()?.parse().map_err(|_| "--part needs a number")?),
            "--lang" => a.lang = Lang::parse(&value()?).ok_or("--lang is de or en")?,
            "--dir" => a.root = value()?.into(),
            _ if arg.starts_with("--") => return Err(format!("unknown option {arg}")),
            _ if a.path.is_none() => a.path = Some(arg),
            _ => return Err("only one file at a time".into()),
        }
    }
    // Own courses (made with AI or imported) are next to the course directories.
    catalog::use_custom_dir(a.root.join(".custom"));
    if let Some(task) = &a.task {
        Lesson::load(Some(task), 1, a.lang)?;
    }
    Ok(a)
}

/// The editor for a file given on the command line. Its lesson follows
/// `--task`, or the file type; in a course directory it goes on where the
/// progress says, unless `--part` says otherwise.
fn editor_for_file(path: String, a: &Args) -> Result<Editor, String> {
    let task = a.task.clone().or_else(|| Lesson::course_for(&path));
    let dir = std::path::absolute(&path)
        .ok()
        .and_then(|p| p.parent().map(Path::to_owned));
    let progress = task
        .as_deref()
        .zip(dir.as_deref())
        .and_then(|(id, dir)| workspace::completed(dir, id));
    let lesson = match (a.part, &task, progress) {
        (None, Some(id), Some(done)) => Lesson::resume(id, done, a.lang)?,
        (part, _, _) => Lesson::load(task.as_deref(), part.unwrap_or(1), a.lang)?,
    };
    let mut editor = Editor::open(path, Some(lesson), a.lang);
    editor.progress_dir = dir.filter(|_| progress.is_some());
    Ok(editor)
}

/// Creates the directory of `course` and opens its starter file at the
/// next lesson.
fn editor_for_course(course: &lesson::Course, root: &Path, lang: Lang) -> Result<Editor, String> {
    let (file, new) = workspace::prepare(root, course, lang).map_err(|e| {
        tr!(
            lang,
            "can't create {}: {e}",
            "kann {} nicht anlegen: {e}",
            workspace::pretty(&root.join(&course.id))
        )
    })?;
    let dir = file
        .parent()
        .expect("is in the course directory")
        .to_owned();
    let done = workspace::completed(&dir, &course.id).unwrap_or(0);
    let lesson = Lesson::resume(&course.id, done, lang)?;
    // Rewritten on every start, so they follow the language.
    workspace::save_progress(&dir, &lesson).map_err(|e| e.to_string())?;
    let mut editor = Editor::open(file.to_string_lossy().into_owned(), Some(lesson), lang);
    let shown = workspace::pretty(&dir);
    editor.status = if new {
        tr!(lang, "[ Created {shown} ]", "[ {shown} angelegt ]")
    } else {
        tr!(lang, "[ Opened {shown} ]", "[ {shown} geöffnet ]")
    };
    editor.progress_dir = Some(dir);
    Ok(editor)
}

/// The start screen: boot once, then pick a course, edit it, and come
/// back to the list when the editor is closed.
fn start_screen(a: &Args) -> io::Result<()> {
    let mut out = io::stdout();
    let mut menu = Menu::new(a.root.clone(), a.lang);
    menu.boot(&mut out)?;
    loop {
        let Choice::Open(course) = menu.pick(&mut out)? else {
            return Ok(());
        };
        match editor_for_course(&course, &menu.root, menu.lang) {
            Ok(mut editor) => {
                run(&mut editor)?;
                menu.status.clear();
            }
            Err(e) => menu.status = format!("[ {e} ]"),
        }
    }
}

fn main() -> io::Result<()> {
    let fail = |e: String| -> ! {
        eprintln!("cbeditor: {e}");
        eprintln!("usage: cbeditor [file] [--task ID] [--part N] [--lang de|en] [--dir DIR]");
        std::process::exit(2);
    };
    let args = parse_args(env::args().skip(1)).unwrap_or_else(|e| fail(e));
    // A file opens directly, so does a course given with --task; otherwise
    // the start screen lists the courses.
    let mut editor = match (&args.path, &args.task) {
        (Some(path), _) => Some(editor_for_file(path.clone(), &args).unwrap_or_else(|e| fail(e))),
        (None, Some(task)) => {
            let course = lesson::courses(args.lang)
                .into_iter()
                .find(|c| &c.id == task)
                .expect("checked by parse_args");
            Some(editor_for_course(&course, &args.root, args.lang).unwrap_or_else(|e| fail(e)))
        }
        (None, None) => None,
    };

    terminal::enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        SetCursorStyle::BlinkingBlock
    )?;
    // Not every terminal can do this; pasting then still works key by key.
    let _ = execute!(io::stdout(), EnableBracketedPaste);
    let result = match &mut editor {
        Some(editor) => run(editor),
        None => start_screen(&args),
    };
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
    let _ = execute!(out, DisableBracketedPaste);
    let _ = out.flush();
    let _ = terminal::disable_raw_mode();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(editor: &mut Editor, code: KeyCode, modifiers: KeyModifiers) -> Action {
        editor.handle(KeyEvent::new(code, modifiers), 10)
    }

    #[test]
    fn altgr_types_braces_like_on_windows() {
        let mut editor = Editor::open("does-not-exist.rs".into(), None, Lang::De);
        let altgr = KeyModifiers::CONTROL | KeyModifiers::ALT;
        for c in ['{', '}', '[', ']', '\\', '|', '@', '~', 'q', 'x', 's'] {
            press(&mut editor, KeyCode::Char(c), altgr);
        }
        assert_eq!(editor.lines, ["{}[]\\|@~qxs"]);
        assert!(!editor.asking, "AltGr+q/x must not ask to exit");
    }

    #[test]
    fn ctrl_shortcuts_still_work() {
        let mut editor = Editor::open("does-not-exist.rs".into(), None, Lang::De);
        press(&mut editor, KeyCode::Char('a'), KeyModifiers::NONE);
        press(&mut editor, KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert!(editor.asking);
        assert!(matches!(
            press(&mut editor, KeyCode::Char('n'), KeyModifiers::NONE),
            Action::Quit
        ));
    }

    fn editor(name: &str) -> Editor {
        Editor::open(format!("does-not-exist/{name}"), None, Lang::De)
    }

    fn type_text(editor: &mut Editor, text: &str) {
        for c in text.chars() {
            let code = if c == '\n' {
                KeyCode::Enter
            } else {
                KeyCode::Char(c)
            };
            press(editor, code, KeyModifiers::NONE);
        }
    }

    #[test]
    fn brackets_close_and_enter_opens_a_block() {
        let mut e = editor("main.rs");
        type_text(&mut e, "fn main() {\nprintln!(\"hi\");");
        assert_eq!(e.lines, ["fn main() {", "    println!(\"hi\");", "}"]);
        assert_eq!((e.row, e.col), (1, 19));
    }

    #[test]
    fn backspace_removes_empty_pairs_and_indent_levels() {
        let mut e = editor("main.rs");
        type_text(&mut e, "        (");
        assert_eq!(e.lines, ["        ()"]);
        press(&mut e, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(e.lines, ["        "]);
        press(&mut e, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(e.lines, ["    "]);
    }

    #[test]
    fn closing_brace_moves_back_a_level() {
        let mut e = editor("main.c");
        e.lines = vec!["    if (x) {".into(), "        ".into()];
        (e.row, e.col) = (1, 8);
        type_text(&mut e, "}");
        assert_eq!(e.lines[1], "    }");
    }

    #[test]
    fn python_colon_indents_and_apostrophes_stay_single() {
        let mut e = editor("main.py");
        type_text(&mut e, "if x:\nprint('don't')");
        assert_eq!(e.lines, ["if x:", "    print('don't')"]);
    }

    #[test]
    fn undo_and_redo_whole_words() {
        let mut e = editor("main.rs");
        type_text(&mut e, "let x");
        press(&mut e, KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(e.lines, ["let "]);
        press(&mut e, KeyCode::Char('z'), KeyModifiers::CONTROL);
        press(&mut e, KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(e.lines, [""]);
        assert!(!e.modified, "back at the file as it was read");
        press(&mut e, KeyCode::Char('y'), KeyModifiers::CONTROL);
        press(&mut e, KeyCode::Char('y'), KeyModifiers::CONTROL);
        press(&mut e, KeyCode::Char('y'), KeyModifiers::CONTROL);
        assert_eq!(e.lines, ["let x"]);
        assert!(e.modified);
    }

    #[test]
    fn pasted_keys_go_in_as_they_are() {
        let mut e = editor("main.rs");
        e.pasting = true;
        type_text(&mut e, "fn f() {\n    x(\"a\");\n}");
        assert_eq!(e.lines, ["fn f() {", "    x(\"a\");", "}"]);

        let mut e = editor("main.rs");
        e.paste("a {\r\n    b\r\n}");
        assert_eq!(e.lines, ["a {", "    b", "}"]);
        press(&mut e, KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(e.lines, [""], "a paste is one undo step");
    }

    #[test]
    fn cut_and_paste_lines() {
        let mut e = editor("main.rs");
        e.lines = vec!["a".into(), "b".into(), "c".into()];
        press(&mut e, KeyCode::Char('k'), KeyModifiers::CONTROL);
        press(&mut e, KeyCode::Char('k'), KeyModifiers::CONTROL);
        assert_eq!(e.lines, ["c"]);
        press(&mut e, KeyCode::End, KeyModifiers::NONE);
        press(&mut e, KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(e.lines, ["a", "b", "c"]);
    }
}
