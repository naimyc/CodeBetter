//! Draws the editor like the CBEditor of the CodeBetter terminal app:
//!
//! ```text
//!  ▓▒░ CBEditor            main.rs                         Geändert   top bar
//!   1 ┆ fn main() {                 ║ 01_rust_basics · …
//!   2 ┆     println!("hi");         ║ ══ LERNEN ════════════        code │ panel
//!                        [ 3 Zeilen gelesen ]                         status
//!  ^S  Speichern  ^X  Beenden  F5  Prüfen  …          Zeile 1, Spalte 1   keys
//! ```
//!
//! Every cell is painted, so the phosphor background fills the screen.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::queue;
use crossterm::style::{Attribute, Print, SetAttribute, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal;

use crate::highlight::Kind;
use crate::lesson::Span;
use crate::theme::{self, Paint};
use crate::tr;
use crate::{Editor, FLASH};

/// Below this terminal width the panel is hidden to leave room for code.
const MIN_WIDTH_FOR_PANEL: usize = 60;
/// Half a blink of a flashing hint.
pub const FLASH_STEP: Duration = Duration::from_millis(125);

/// Rows left for code between the top bar, status line and key bar.
pub fn text_height(screen_h: usize) -> usize {
    screen_h.saturating_sub(3).max(1)
}

impl Editor {
    pub fn draw(&mut self, out: &mut impl Write) -> io::Result<()> {
        let (w, h) = terminal::size()?;
        // Lay out at least the bars and one code row, even in a tiny window.
        let (w, h) = (w as usize, (h as usize).max(4));
        let text_h = text_height(h);

        // Split the screen: code on the left, lesson panel on the right.
        let panel_w = match &self.lesson {
            Some(_) if self.show_panel && w >= MIN_WIDTH_FOR_PANEL => (w * 42 / 100).min(85),
            _ => 0,
        };
        let code_w = w - panel_w;
        self.panel_x = (panel_w > 0).then_some(code_w);
        let panel = match &self.lesson {
            // The border and one column of padding on each side.
            Some(lesson) if panel_w > 0 => lesson.render(panel_w - 3, self.flashing()),
            _ => Vec::new(),
        };
        self.panel_top = self.panel_top.min(panel.len().saturating_sub(text_h));

        let digits = self.lines.len().to_string().len().max(2);
        // " 12 ┆ " before the code.
        let gutter_w = digits + 4;
        let text_w = code_w.saturating_sub(gutter_w).max(1);
        self.scroll(text_w, text_h);

        let kinds = self.syntax.highlight_all(&self.lines);
        let brackets = self.matching_brackets(&kinds);
        // A bracket without a partner shows up red.
        let bracket = if brackets.len() == 1 {
            Paint::new(theme::BG, theme::RED).bold()
        } else {
            Paint::new(theme::FG_BRIGHT, theme::MATCH_BG).bold()
        };

        queue!(out, Hide)?;
        self.draw_top_bar(out, w)?;
        for y in 0..text_h {
            queue!(out, MoveTo(0, (y + 1) as u16))?;
            let row = self.top + y;
            let current = row == self.row;
            let bg = if current {
                theme::CURRENT_LINE
            } else {
                theme::BG
            };
            let mut spans: Vec<Span> = match self.lines.get(row) {
                Some(_) => {
                    let number = Paint::new(if current { theme::FG } else { theme::FG_DIM }, bg);
                    vec![(format!(" {:>digits$} ", row + 1), number)]
                }
                None => vec![(" ".repeat(digits + 2), theme::GUTTER)],
            };
            spans.push(("┆ ".into(), Paint::new(theme::FG_DIM, bg)));
            if let Some(line) = self.lines.get(row) {
                let chars = line.chars().enumerate().skip(self.left).take(text_w);
                for (col, c) in chars {
                    let paint = if brackets.contains(&(row, col)) {
                        bracket
                    } else {
                        theme::code(kinds[row][col], bg)
                    };
                    match spans.last_mut() {
                        Some((text, last)) if *last == paint => text.push(c),
                        _ => spans.push((c.into(), paint)),
                    }
                }
            }
            put_line(out, &spans, code_w, Paint::new(theme::FG, bg))?;
            if panel_w > 0 {
                put(out, "║ ", 2, Paint::new(theme::FG, theme::PANEL_BG))?;
                let line = panel.get(self.panel_top + y).map_or(&[][..], |l| &l[..]);
                put_line(out, line, panel_w - 2, theme::PANEL)?;
            }
        }

        let status_paint = if self.asking {
            theme::ASK
        } else {
            theme::STATUS
        };
        queue!(out, MoveTo(0, (h - 2) as u16))?;
        put(out, &center(&self.status, w), w, status_paint)?;
        self.draw_key_bar(out, w, (h - 1) as u16)?;

        let cursor_x = gutter_w + self.col - self.left;
        let cursor_y = 1 + self.row - self.top;
        queue!(out, MoveTo(cursor_x as u16, cursor_y as u16), Show)?;
        out.flush()
    }

    /// `▓▒░ CBEditor`, the file name in the middle, `Geändert` on the right.
    fn draw_top_bar(&self, out: &mut impl Write, w: usize) -> io::Result<()> {
        let brand = " ▓▒░ CBEditor";
        let modified = if self.modified {
            self.lang.pick("Modified ", "Geändert ")
        } else {
            ""
        };
        let name_len = self.name.chars().count();
        let name_at = (w.saturating_sub(name_len) / 2).max(brand.chars().count() + 2);
        let gap = name_at - brand.chars().count();
        let right = w.saturating_sub(name_at + name_len + modified.chars().count());
        queue!(out, MoveTo(0, 0))?;
        put_line(
            out,
            &[
                (brand.into(), theme::BAR.bold()),
                (" ".repeat(gap), theme::BAR),
                (self.name.clone(), theme::BAR),
                (" ".repeat(right), theme::BAR),
                (modified.into(), theme::BAR),
            ],
            w,
            theme::BAR,
        )
    }

    /// The shortcuts as `[^S] Speichern`, the cursor position on the right.
    fn draw_key_bar(&self, out: &mut impl Write, w: usize, y: u16) -> io::Result<()> {
        let lang = self.lang;
        let keys = [
            ("^S", lang.pick("Save", "Speichern")),
            ("^X", lang.pick("Exit", "Beenden")),
            ("^Z", lang.pick("Undo", "Rückgängig")),
            ("F5", lang.pick("Check", "Prüfen")),
            ("F2", lang.pick("Hint", "Tipp")),
            ("F1", "Panel"),
            ("Tab", lang.pick("Indent", "Einrücken")),
        ];
        let mut spans: Vec<Span> = vec![(" ".into(), theme::BAR)];
        for (key, label) in keys {
            spans.push((format!(" {key} "), theme::KEY));
            spans.push((format!(" {label}  "), theme::BAR));
        }
        let pos = tr!(
            lang,
            "line {}, col {} ",
            "Zeile {}, Spalte {} ",
            self.row + 1,
            self.col + 1
        );
        let pos_w = pos.chars().count().min(w);
        queue!(out, MoveTo(0, y))?;
        put_line(out, &spans, w - pos_w, theme::BAR)?;
        put(out, &pos, pos_w, theme::BAR)
    }

    /// The bracket at (or just before) the cursor and its partner, as
    /// (row, col). Brackets in strings and comments don't count.
    fn matching_brackets(&self, kinds: &[Vec<Kind>]) -> Vec<(usize, usize)> {
        const OPEN: &str = "([{";
        const CLOSE: &str = ")]}";
        let code = |row: usize, col: usize| {
            let c = self.lines[row].chars().nth(col)?;
            let plain = !matches!(kinds[row].get(col), Some(Kind::Str | Kind::Comment));
            plain.then_some(c)
        };
        let is_bracket = |c: char| OPEN.contains(c) || CLOSE.contains(c);
        let at = [Some(self.col), self.col.checked_sub(1)]
            .into_iter()
            .flatten()
            .find_map(|col| {
                code(self.row, col)
                    .filter(|c| is_bracket(*c))
                    .map(|c| (col, c))
            });
        let Some((col, c)) = at else {
            return Vec::new();
        };
        let forward = OPEN.contains(c);
        let (open, close) = if forward {
            let i = OPEN.find(c).expect("is an opener");
            (c, CLOSE.as_bytes()[i] as char)
        } else {
            let i = CLOSE.find(c).expect("is a closer");
            (OPEN.as_bytes()[i] as char, c)
        };
        let (mut row, mut x, mut depth) = (self.row, col, 0usize);
        loop {
            match code(row, x) {
                Some(ch) if ch == open || ch == close => {
                    if (ch == open) == forward {
                        depth += 1;
                    } else {
                        depth -= 1;
                        if depth == 0 {
                            return vec![(self.row, col), (row, x)];
                        }
                    }
                }
                _ => {}
            }
            // Step one char forward or back, across line ends.
            if forward {
                if x + 1 < self.line_len(row) {
                    x += 1;
                } else if row + 1 < self.lines.len() {
                    row += 1;
                    x = 0;
                } else {
                    return vec![(self.row, col)];
                }
            } else if x > 0 {
                x -= 1;
            } else if row > 0 {
                row -= 1;
                x = self.line_len(row).saturating_sub(1);
            } else {
                return vec![(self.row, col)];
            }
        }
    }

    /// Whether the newest hint is in the highlighted half of a blink.
    fn flashing(&self) -> bool {
        self.flash_until.is_some_and(|until| {
            let elapsed = FLASH.saturating_sub(until.saturating_duration_since(Instant::now()));
            (elapsed.as_millis() / FLASH_STEP.as_millis()).is_multiple_of(2)
        })
    }

    /// Scrolls so the cursor stays inside a `width` x `height` text area.
    fn scroll(&mut self, width: usize, height: usize) {
        if self.row < self.top {
            self.top = self.row;
        } else if self.row >= self.top + height {
            self.top = self.row + 1 - height;
        }
        if self.col < self.left {
            self.left = self.col;
        } else if self.col >= self.left + width {
            self.left = self.col + 1 - width;
        }
    }
}

/// `text` centered in `width` columns.
pub fn center(text: &str, width: usize) -> String {
    let pad = width.saturating_sub(text.chars().count()) / 2;
    format!("{}{text}", " ".repeat(pad))
}

/// Prints `text` cut or padded to exactly `width` columns.
pub fn put(out: &mut impl Write, text: &str, width: usize, paint: Paint) -> io::Result<()> {
    let text: String = text.chars().take(width).collect();
    let pad = width - text.chars().count();
    let weight = if paint.bold {
        Attribute::Bold
    } else {
        Attribute::NormalIntensity
    };
    queue!(
        out,
        SetForegroundColor(paint.fg),
        SetBackgroundColor(paint.bg),
        SetAttribute(weight),
        Print(text),
        Print(" ".repeat(pad)),
    )
}

/// Prints the spans, cut at `width`, and fills the rest with `fill`.
pub fn put_line(out: &mut impl Write, spans: &[Span], width: usize, fill: Paint) -> io::Result<()> {
    let mut left = width;
    for (text, paint) in spans {
        let n = text.chars().count().min(left);
        put(out, text, n, *paint)?;
        left -= n;
    }
    put(out, "", left, fill)
}
