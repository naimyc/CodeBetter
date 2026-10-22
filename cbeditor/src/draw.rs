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

        queue!(out, Hide)?;
        self.draw_top_bar(out, w)?;
        for y in 0..text_h {
            queue!(out, MoveTo(0, (y + 1) as u16))?;
            let row = self.top + y;
            let number = match self.lines.get(row) {
                Some(_) => format!(" {:>digits$} ", row + 1),
                None => " ".repeat(digits + 2),
            };
            let code: String = self.lines.get(row).map_or(String::new(), |line| {
                line.chars().skip(self.left).take(text_w).collect()
            });
            put_line(
                out,
                &[
                    (number, theme::GUTTER),
                    ("┆ ".into(), theme::GUTTER),
                    (code, theme::CODE),
                ],
                code_w,
                theme::CODE,
            )?;
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
fn center(text: &str, width: usize) -> String {
    let pad = width.saturating_sub(text.chars().count()) / 2;
    format!("{}{text}", " ".repeat(pad))
}

/// Prints `text` cut or padded to exactly `width` columns.
fn put(out: &mut impl Write, text: &str, width: usize, paint: Paint) -> io::Result<()> {
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
fn put_line(out: &mut impl Write, spans: &[Span], width: usize, fill: Paint) -> io::Result<()> {
    let mut left = width;
    for (text, paint) in spans {
        let n = text.chars().count().min(left);
        put(out, text, n, *paint)?;
        left -= n;
    }
    put(out, "", left, fill)
}
