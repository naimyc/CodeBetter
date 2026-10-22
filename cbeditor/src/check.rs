//! F5: compiles the open file, runs a lesson's tests on it and checks that
//! the code uses the lesson's concept. Ported from `submit` in the
//! CodeBetter terminal app; the report goes to the PRÜFUNG pane.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::i18n::{Lang, Text};
use crate::runner;
use crate::theme::{self, Paint};
use crate::tr;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
pub struct TestCase {
    #[serde(default)]
    input: Text,
    expected: Text,
    #[serde(default)]
    hidden: bool,
}

/// Something the source code must contain, e.g. `let mut `.
#[derive(Deserialize)]
pub struct Requirement {
    code: String,
    message: Text,
}

/// One line of the report.
pub type ReportLine = (String, Paint);

pub const OK: Paint = Paint::new(theme::FG, theme::PANEL_BG);
pub const BAD: Paint = Paint::new(theme::RED, theme::PANEL_BG);
pub const WARN: Paint = Paint::new(theme::YELLOW, theme::PANEL_BG);
pub const NOTE: Paint = Paint::new(theme::NOTE, theme::PANEL_BG);
const TEXT: Paint = Paint::new(theme::FG_BRIGHT, theme::PANEL_BG);

/// Runs `tests` and `requires` against `source`. Returns the report and
/// whether everything passed.
pub fn check(
    source: &Path,
    tests: &[TestCase],
    requires: &[Requirement],
    lang: Lang,
) -> (Vec<ReportLine>, bool) {
    let mut r = Vec::new();
    let name = source.file_name().unwrap_or_default().to_string_lossy();
    r.push((
        tr!(lang, "» building {name} ...", "» baue {name} ..."),
        NOTE,
    ));
    let program = match runner::prepare(source, lang) {
        Ok(program) => {
            push_block(&mut r, &program.compile_log, NOTE);
            program
        }
        Err(log) => {
            push_block(&mut r, &log, BAD);
            r.push((
                tr!(lang, "✗ build failed", "✗ Build fehlgeschlagen"),
                BAD.bold(),
            ));
            return (r, false);
        }
    };

    let mut failure = Vec::new();
    let mut passed = 0;
    for (i, test) in tests.iter().enumerate() {
        let n = i + 1;
        let input = test.input.get(lang);
        let res = match program.run(input, TEST_TIMEOUT, lang) {
            Ok(res) => res,
            Err(e) => {
                r.push((e, BAD));
                return (r, false);
            }
        };
        let label = if test.hidden {
            tr!(lang, " (hidden)", " (versteckt)")
        } else {
            String::new()
        };
        let got = runner::normalize(&res.stdout);
        let want = runner::normalize(test.expected.get(lang));
        let problem = if res.timed_out {
            let secs = TEST_TIMEOUT.as_secs();
            Some(tr!(
                lang,
                "time limit of {secs}s exceeded. Endless loop?",
                "Zeitlimit von {secs}s überschritten. Endlosschleife?"
            ))
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
        let Some(why) = problem else {
            passed += 1;
            r.push((format!("  test {n}{label:<12} ok"), OK));
            continue;
        };
        let failed = tr!(lang, "FAILED", "FEHLGESCHLAGEN");
        r.push((format!("  test {n}{label:<12} {failed}"), BAD));
        if !failure.is_empty() {
            continue;
        }
        failure.push((String::new(), TEXT));
        let head = tr!(lang, "test {n} failed: ", "Test {n} fehlgeschlagen: ");
        push_block(&mut failure, &format!("{head}{why}"), BAD.bold());
        if test.hidden {
            failure.push((
                tr!(
                    lang,
                    "  this test is hidden. Think about edge cases the examples don't show.",
                    "  dieser Test ist versteckt. Denk an Randfälle, die die Beispiele nicht zeigen."
                ),
                NOTE,
            ));
        } else {
            let (l_in, l_want, l_got) = match lang {
                Lang::En => ("input", "expected", "got"),
                Lang::De => ("Eingabe", "erwartet", "erhalten"),
            };
            for (label, text) in [(l_in, input), (l_want, &want), (l_got, &got)] {
                failure.push((format!("  {label}:"), NOTE));
                indent_block(&mut failure, text, lang);
            }
        }
    }

    let total = tests.len();
    if !failure.is_empty() {
        r.extend(failure);
        r.push((String::new(), TEXT));
        r.push((
            tr!(
                lang,
                "{passed}/{total} passed. Not quite yet.",
                "{passed}/{total} bestanden. Noch nicht ganz."
            ),
            BAD,
        ));
        r.push((
            tr!(
                lang,
                "Stuck? Press F2 for a hint.",
                "Hängst du fest? F2 zeigt einen Tipp."
            ),
            NOTE,
        ));
        return (r, false);
    }

    let code = std::fs::read_to_string(source).unwrap_or_default();
    let missing = missing_requirements(requires, &code);
    if !missing.is_empty() {
        r.push((String::new(), TEXT));
        r.push((
            tr!(
                lang,
                "Output is correct, but the code doesn't use what this lesson is about yet:",
                "Die Ausgabe stimmt, aber der Code benutzt noch nicht, worum es in dieser Lektion geht:"
            ),
            WARN.bold(),
        ));
        for req in missing {
            r.push((format!("  » {}", req.message.get(lang)), WARN));
        }
        return (r, false);
    }
    (r, true)
}

/// Requirements the source doesn't meet. Comments don't count, and runs
/// of whitespace compare as one space.
fn missing_requirements<'a>(requires: &'a [Requirement], source: &str) -> Vec<&'a Requirement> {
    let code: String = source
        .lines()
        .map(|l| l.find("//").map_or(l, |i| &l[..i]))
        .collect::<Vec<_>>()
        .join(" ");
    let code = collapse_whitespace(&code);
    requires
        .iter()
        .filter(|r| !code.contains(&collapse_whitespace(&r.code)))
        .collect()
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    if space {
        out.push(' ');
    }
    out
}

fn push_block(r: &mut Vec<ReportLine>, text: &str, paint: Paint) {
    r.extend(text.trim_end().lines().map(|l| (l.to_owned(), paint)));
}

fn indent_block(r: &mut Vec<ReportLine>, text: &str, lang: Lang) {
    if text.is_empty() {
        r.push((format!("    {}", lang.pick("(empty)", "(leer)")), NOTE));
    }
    r.extend(text.lines().map(|l| (format!("    {l}"), TEXT)));
}

/// `[#####.....]` for `done` of `total`.
pub fn progress_bar(done: usize, total: usize, width: usize) -> String {
    let filled = (done * width).checked_div(total).unwrap_or(width);
    format!("[{}{}]", "#".repeat(filled), ".".repeat(width - filled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirements_ignore_comments_and_spacing() {
        let req = |code: &str| Requirement {
            code: code.into(),
            message: Text::None,
        };
        let reqs = [req("let mut "), req("match ")];
        let src = "fn main() {\n    let   mut x = 1; // match x\n}";
        let missing = missing_requirements(&reqs, src);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].code, "match ");
    }
}
