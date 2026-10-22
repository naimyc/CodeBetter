//! The lesson panel: reads the CodeBetter course catalog and lays out one
//! lesson (theory, exercise, revealed hints, check result) as colored lines
//! for the side panel, styled like the panel in the CodeBetter terminal app.

use std::path::Path;

use serde::Deserialize;

use crate::check::{self, ReportLine, Requirement, TestCase};
use crate::i18n::{Lang, Text};
use crate::theme::{self, Paint};
use crate::tr;

/// Same catalog the Tauri app uses, compiled in.
const CATALOG: &str = include_str!("../../codebetter-terminal/src-tauri/tasks/tasks.toml");

#[derive(Deserialize)]
struct Catalog {
    #[serde(rename = "task")]
    tasks: Vec<Task>,
}

#[derive(Deserialize)]
struct Task {
    id: String,
    title: Text,
    #[serde(rename = "part")]
    parts: Vec<Part>,
}

#[derive(Deserialize)]
struct Part {
    title: Text,
    #[serde(default)]
    lesson: Text,
    description: Text,
    #[serde(default)]
    hints: Vec<Text>,
    #[serde(default, rename = "require")]
    requires: Vec<Requirement>,
    #[serde(rename = "test")]
    tests: Vec<TestCase>,
}

/// What appeared last and flashes in the panel.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fresh {
    Nothing,
    Hint,
    Lesson,
}

/// One lesson of one course, as shown in the panel.
pub struct Lesson {
    task: Task,
    part: usize,
    lang: Lang,
    hints_shown: usize,
    /// Every lesson passed.
    done: bool,
    /// The last F5 result; `None` before the first check.
    report: Option<Vec<ReportLine>>,
    fresh: Fresh,
}

impl Lesson {
    /// Loads lesson `part` (1-based) of course `task_id`, or of the first
    /// course when no id is given.
    pub fn load(task_id: Option<&str>, part: usize, lang: Lang) -> Result<Self, String> {
        let catalog: Catalog =
            toml::from_str(CATALOG).map_err(|e| format!("tasks.toml is invalid: {e}"))?;
        let task = match task_id {
            Some(id) => catalog.tasks.into_iter().find(|t| t.id == id),
            None => catalog.tasks.into_iter().next(),
        }
        .ok_or_else(|| format!("no course {}", task_id.unwrap_or("in the catalog")))?;
        if part == 0 || part > task.parts.len() {
            return Err(format!("{} has lessons 1 to {}", task.id, task.parts.len()));
        }
        Ok(Self {
            task,
            part: part - 1,
            lang,
            hints_shown: 0,
            done: false,
            report: None,
            fresh: Fresh::Nothing,
        })
    }

    fn current(&self) -> &Part {
        &self.task.parts[self.part]
    }

    /// Reveals one more hint. Returns false when all are already shown.
    pub fn reveal_hint(&mut self) -> bool {
        if !self.done && self.hints_shown < self.current().hints.len() {
            self.hints_shown += 1;
            self.fresh = Fresh::Hint;
            true
        } else {
            false
        }
    }

    /// Shows "checking ..." until `check` has a result.
    pub fn start_check(&mut self) {
        let checking = self.lang.pick("checking ...", "prüfe ...");
        self.report = Some(vec![(checking.into(), check::NOTE)]);
    }

    /// F5: tests `source` against the current lesson. On a pass the next
    /// lesson is unlocked; returns whether it passed.
    pub fn check(&mut self, source: &Path) -> bool {
        let lang = self.lang;
        if self.done {
            let done = lang.pick(
                "This course is already complete.",
                "Dieser Kurs ist schon geschafft.",
            );
            self.report = Some(vec![(done.into(), check::OK)]);
            return false;
        }
        let part = self.current();
        let (mut report, passed) = check::check(source, &part.tests, &part.requires, lang);
        if passed {
            let total = self.task.parts.len();
            let completed = self.part + 1;
            let bar = check::progress_bar(completed, total, 20);
            let n = part.tests.len();
            report.push((String::new(), check::OK));
            let all = tr!(lang, "✓ all {n} tests passed", "✓ alle {n} Tests bestanden");
            report.push((format!("{all}  {bar}"), check::OK.bold()));
            if completed < total {
                self.part += 1;
                self.fresh = Fresh::Lesson;
                let next = lang.pick("» next lesson unlocked", "» nächste Lektion freigeschaltet");
                report.push((next.into(), check::WARN));
            } else {
                self.done = true;
                report.push((String::new(), check::OK));
                report.push((
                    lang.pick("  ★ COURSE COMPLETE ★", "  ★ KURS GESCHAFFT ★")
                        .into(),
                    check::OK.bold(),
                ));
                let msg = lang.pick(
                    "  You wrote every line of it yourself. That's the point.",
                    "  Jede Zeile davon hast du selbst geschrieben. Genau darum geht's.",
                );
                report.push((msg.into(), check::OK));
            }
            self.hints_shown = 0;
        }
        self.report = Some(report);
        passed
    }

    /// The panel content, wrapped to `width`. With `flash`, whatever just
    /// appeared (a hint or the next lesson) is highlighted.
    pub fn render(&self, width: usize, flash: bool) -> Vec<Line> {
        let lang = self.lang;
        let part = self.current();
        let mut out = Vec::new();
        let title = format!("{} · {}", self.task.id, self.task.title.get(lang));
        wrap_into(&mut out, &title, width, PANEL_BRIGHT.bold());
        if self.done {
            let complete = lang.pick("★ complete", "★ geschafft");
            wrap_into(
                &mut out,
                complete,
                width,
                Paint::new(theme::BLUE, theme::PANEL_BG),
            );
            pane_title(&mut out, lang.pick("YOUR TASK", "DEINE AUFGABE"), width);
            let text = lang.pick(
                "All lessons passed. You wrote every line of this program yourself.",
                "Alle Lektionen bestanden. Jede Zeile dieses Programms hast du selbst geschrieben.",
            );
            wrap_into(&mut out, text, width, theme::PANEL);
            self.render_report(&mut out, width);
            return out;
        }
        let count = tr!(
            lang,
            "Lesson {}/{}",
            "Lektion {}/{}",
            self.part + 1,
            self.task.parts.len()
        );
        let part_title = format!("{count} · {}", part.title.get(lang));
        let part_paint = if flash && self.fresh == Fresh::Lesson {
            theme::FLASH
        } else {
            Paint::new(theme::BLUE, theme::PANEL_BG)
        };
        wrap_into(&mut out, &part_title, width, part_paint);

        let theory = part.lesson.get(lang).trim();
        if !theory.is_empty() {
            pane_title(&mut out, lang.pick("LEARN", "LERNEN"), width);
            wrap_into(
                &mut out,
                theory,
                width,
                Paint::new(theme::LESSON, theme::PANEL_BG),
            );
        }

        pane_title(&mut out, lang.pick("YOUR TASK", "DEINE AUFGABE"), width);
        wrap_into(
            &mut out,
            part.description.get(lang).trim(),
            width,
            theme::PANEL,
        );

        let total = part.hints.len();
        pane_title(
            &mut out,
            &tr!(lang, "HINTS {}/{}", "TIPPS {}/{}", self.hints_shown, total),
            width,
        );
        for (i, hint) in part.hints.iter().take(self.hints_shown).enumerate() {
            let newest = flash && self.fresh == Fresh::Hint && i + 1 == self.hints_shown;
            let paint = if newest { theme::FLASH } else { HINT };
            hint_into(&mut out, i + 1, hint.get(lang).trim(), width, paint);
        }
        let note = if total == 0 {
            lang.pick(
                "No hints for this lesson.",
                "Für diese Lektion gibt es keine Tipps.",
            )
        } else if self.hints_shown < total {
            lang.pick(
                "F2 reveals the next hint.",
                "F2 deckt den nächsten Tipp auf.",
            )
        } else {
            lang.pick(
                "All hints revealed. You've got this.",
                "Alle Tipps aufgedeckt. Du schaffst das.",
            )
        };
        wrap_into(
            &mut out,
            note,
            width,
            Paint::new(theme::NOTE, theme::PANEL_BG),
        );
        self.render_report(&mut out, width);
        out
    }

    /// The PRÜFUNG pane: the last F5 result, or how to start one.
    fn render_report(&self, out: &mut Vec<Line>, width: usize) {
        let lang = self.lang;
        pane_title(out, lang.pick("CHECK", "PRÜFUNG"), width);
        match &self.report {
            Some(report) => {
                for (text, paint) in report {
                    wrap_into(out, text, width, *paint);
                }
            }
            None => {
                let idle = lang.pick(
                    "F5 saves and checks your solution against the tests.",
                    "F5 speichert und prüft deine Lösung gegen die Tests.",
                );
                wrap_into(out, idle, width, check::NOTE);
            }
        }
    }
}

const PANEL_BRIGHT: Paint = Paint::new(theme::FG_BRIGHT, theme::PANEL_BG);
const HINT: Paint = Paint::new(theme::YELLOW, theme::PANEL_BG);

/// A run of text in one paint.
pub type Span = (String, Paint);
/// One panel row; its spans together are at most the panel width.
pub type Line = Vec<Span>;

/// A yellow `══ TITLE ═════` rule, after a blank line separating panes.
fn pane_title(out: &mut Vec<Line>, title: &str, width: usize) {
    let head = format!("══ {title} ");
    let fill = "═".repeat(width.saturating_sub(head.chars().count()));
    out.push(Vec::new());
    out.push(vec![(head + &fill, HINT)]);
}

/// A numbered hint: dim `1.` marker, text indented under it.
fn hint_into(out: &mut Vec<Line>, n: usize, text: &str, width: usize, paint: Paint) {
    let marker = format!("{n}. ");
    let indent = marker.len();
    let mut lines = Vec::new();
    wrap_into(&mut lines, text, width.saturating_sub(indent), paint);
    for (i, mut line) in lines.into_iter().enumerate() {
        let lead = if i == 0 {
            marker.clone()
        } else {
            " ".repeat(indent)
        };
        line.insert(0, (lead, Paint::new(theme::FG_DIM, theme::PANEL_BG)));
        out.push(line);
    }
}

/// Word-wraps `text` to `width` columns. Lines that fit stay as they are;
/// continuation lines keep the indentation of their source line, so code
/// examples stay readable.
fn wrap_into(out: &mut Vec<Line>, text: &str, width: usize, paint: Paint) {
    let width = width.max(8);
    let mut push = |line: String| out.push(vec![(line, paint)]);
    if text.is_empty() {
        push(String::new());
    }
    for src in text.lines() {
        if src.chars().count() <= width {
            push(src.to_owned());
            continue;
        }
        let indent = src.len() - src.trim_start().len();
        let pad = " ".repeat(indent.min(width / 2));
        let mut line = pad.clone();
        let mut len = pad.len();
        for word in src.split_whitespace() {
            if len > pad.len() {
                if len + 1 + word.chars().count() > width {
                    push(std::mem::replace(&mut line, pad.clone()));
                    len = pad.len();
                } else {
                    line.push(' ');
                    len += 1;
                }
            }
            // A single word longer than the panel gets hard-split.
            let mut rest: Vec<char> = word.chars().collect();
            while len + rest.len() > width {
                line.extend(rest.drain(..width - len));
                push(std::mem::replace(&mut line, pad.clone()));
                len = pad.len();
            }
            len += rest.len();
            line.extend(rest);
        }
        push(line);
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
    fn wraps_at_word_boundaries_and_keeps_indent() {
        let mut out = Vec::new();
        wrap_into(
            &mut out,
            "one two three four\n    let x = five six;",
            10,
            theme::PANEL,
        );
        assert_eq!(
            texts(out),
            [
                "one two",
                "three four",
                "    let x",
                "    = five",
                "    six;"
            ]
        );
    }

    /// Writes `code` to a scratch file and checks it against lesson `part`.
    fn check_code(part: usize, code: &str, name: &str) -> (Lesson, bool) {
        let dir = std::env::temp_dir().join(format!("cbeditor-test-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("main.rs");
        std::fs::write(&file, code).unwrap();
        let mut lesson = Lesson::load(None, part, Lang::De).unwrap();
        let passed = lesson.check(&file);
        (lesson, passed)
    }

    #[test]
    fn reference_solutions_pass_and_unlock_the_next_lesson() {
        let dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../codebetter-terminal/src-tauri/tasks/solutions/01_rust_basics"
        );
        for part in 1..=10 {
            let code = std::fs::read_to_string(format!("{dir}/part{part}.rs")).unwrap();
            let (lesson, passed) = check_code(part, &code, &format!("part{part}"));
            let report = texts(lesson.render(200, false)).join("\n");
            assert!(passed, "lesson {part} failed:\n{report}");
            assert_eq!(lesson.done, part == 10);
            if part < 10 {
                assert_eq!(
                    lesson.part, part,
                    "lesson {part} didn't unlock the next one"
                );
            }
        }
    }

    #[test]
    fn wrong_output_fails_and_shows_the_difference() {
        let (lesson, passed) = check_code(1, r#"fn main() { println!("hallo"); }"#, "wrong");
        assert!(!passed);
        assert_eq!(lesson.part, 0);
        let report = texts(lesson.render(200, false)).join("\n");
        assert!(report.contains("FEHLGESCHLAGEN"), "{report}");
        assert!(report.contains("erhalten:\n    hallo"), "{report}");
    }

    #[test]
    fn build_errors_are_reported() {
        let (lesson, passed) = check_code(1, "fn main() { nope }", "broken");
        assert!(!passed);
        let report = texts(lesson.render(200, false)).join("\n");
        assert!(report.contains("Build fehlgeschlagen"), "{report}");
    }

    #[test]
    fn every_lesson_renders_in_both_languages() {
        let catalog: Catalog = toml::from_str(CATALOG).unwrap();
        for task in &catalog.tasks {
            for part in 1..=task.parts.len() {
                for lang in [Lang::De, Lang::En] {
                    let mut lesson = Lesson::load(Some(&task.id), part, lang).unwrap();
                    while lesson.reveal_hint() {}
                    for line in texts(lesson.render(30, true)) {
                        assert!(line.chars().count() <= 30, "too wide: {line:?}");
                    }
                }
            }
        }
    }
}
