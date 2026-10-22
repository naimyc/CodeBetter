//! The green phosphor palette of the CodeBetter terminal app (ui/style.css).

use crossterm::style::Color;

const fn rgb(hex: u32) -> Color {
    Color::Rgb {
        r: (hex >> 16) as u8,
        g: (hex >> 8) as u8,
        b: hex as u8,
    }
}

pub const BG: Color = rgb(0x040804);
pub const FG: Color = rgb(0x33ff66);
pub const FG_BRIGHT: Color = rgb(0xb8ffcc);
pub const FG_DIM: Color = rgb(0x1c8c3e);
pub const YELLOW: Color = rgb(0xffc23a);
pub const RED: Color = rgb(0xff5a4f);
pub const BLUE: Color = rgb(0x5fd7ff);
/// The panel's faint green tint: rgba(51, 255, 102, 0.035) over BG.
pub const PANEL_BG: Color = rgb(0x061107);
/// The lesson text: FG_BRIGHT at 85% opacity on PANEL_BG.
pub const LESSON: Color = rgb(0x9ddbae);
/// Dimmed notes (`.d`): FG at 60% opacity on PANEL_BG.
pub const NOTE: Color = rgb(0x21a040);

/// Colors and weight for a run of text.
#[derive(Clone, Copy)]
pub struct Paint {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
}

impl Paint {
    pub const fn new(fg: Color, bg: Color) -> Self {
        Self {
            fg,
            bg,
            bold: false,
        }
    }

    pub const fn bold(self) -> Self {
        Self { bold: true, ..self }
    }
}

pub const CODE: Paint = Paint::new(FG, BG);
pub const GUTTER: Paint = Paint::new(FG_DIM, BG);
/// The inverted top and bottom bars.
pub const BAR: Paint = Paint::new(BG, FG);
/// A key like `^S` inside a bar.
pub const KEY: Paint = Paint::new(FG, BG);
pub const STATUS: Paint = Paint::new(FG_BRIGHT, BG);
/// The status line while asking a question.
pub const ASK: Paint = Paint::new(BG, YELLOW);
pub const PANEL: Paint = Paint::new(FG, PANEL_BG);
/// A line that just appeared, highlighted while it flashes.
pub const FLASH: Paint = Paint::new(BG, FG);
