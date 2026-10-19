# CodeBetter Terminal

A retro, CRT-style terminal for CodeBetter (Tauri + Rust), built like a
Codecademy course. You navigate a small Linux-like filesystem and open a
course in `~/Tasks.cb`. Every lesson first teaches a concept (LERNEN),
then gives you an exercise (DEINE AUFGABE). You extend **one program** lesson by
lesson. `submit` compiles it, runs the tests, checks that you used the
concept, and unlocks the next lesson.

```
dev@codebetter:~$ cd Tasks.cb/01_rust_basics
dev@codebetter:~/Tasks.cb/01_rust_basics$ edit main.rs     # CBEditor: code left, lesson right
dev@codebetter:~/Tasks.cb/01_rust_basics$ submit
SUBMIT 01_rust_basics · Lektion 3/10 · Rechnen und mut
  test 1              ok
✓ alle 1 Tests bestanden  [######..............]
» nächste Lektion freigeschaltet: cat part4.md
```

## Run

```bash
cd src-tauri
cargo tauri dev        # development
cargo tauri build      # release bundle
cargo test             # shell tests + every reference solution
```

Requirements: Rust, `cargo tauri` (tauri-cli 2), webkit2gtk 4.1. To run
solutions you need `rustc`, and optionally `python3`, `node` or `cc` for
`.py`, `.js` or `.c` files.

## The Rust course: CB Shop

Ten lessons for complete beginners. Together they build the till of a small
shop, all in the same `main.rs`:

| # | Lesson | What the program learns |
|---|--------|-------------------------|
| 1 | Hallo, Rust! | print the heading with `println!` |
| 2 | Variablen | item and price with `let` and `{}` |
| 3 | Rechnen und mut | quantity and total with `let mut` |
| 4 | Funktionen | refactor into `fn gesamtpreis(...) -> u32` |
| 5 | Eingaben lesen | read the quantity with `read_line` + `parse` |
| 6 | Entscheidungen mit if | 1 Euro discount from 10 Euro on |
| 7 | match | price list `preis_von` for Apfel, Brot, Milch |
| 8 | Schleifen | many items, one line each, with `for` |
| 9 | Structs und Vec | a `struct Posten` collected in a `Vec` |
| 10 | Fehler behandeln | `Option`/`Result`: unknown items and bad input never crash |

## How it works

| Part | File |
|------|------|
| Shell: commands, paths, redirection, tab completion | `src-tauri/src/shell.rs` |
| Courses, progress, code checks, writing `Tasks.cb` | `src-tauri/src/tasks.rs` |
| Compile and run with a time limit | `src-tauri/src/runner.rs` |
| Languages, `tr!` macro, settings | `src-tauri/src/i18n.rs` |
| Course content (de + en) | `src-tauri/tasks/tasks.toml` |
| Reference solution per lesson | `src-tauri/tasks/solutions/<course>/partN.rs` |
| Terminal UI | `ui/terminal.js` |
| CBEditor | `ui/cbeditor.js` |

* **Filesystem.** `/` in the terminal is a real directory in the app data
  folder (`~/.local/share/com.codebetter.terminal/fs` on Linux). Paths can't
  leave it. Your code is a normal file, so you can also open it in your own
  editor.
* **Lessons unlock step by step.** Only `part1.md` exists at first. A
  successful `submit` writes the next `partN.md` and updates the course
  `README.md`. Progress is stored in `progress.json` next to `fs/`.
* **Tests** feed `input` to stdin and compare stdout with `expected`.
  Trailing whitespace doesn't count. Hidden tests don't show their input
  when they fail.
* **Code checks.** After the output is right, `submit` checks that the
  code uses the lesson's concept (e.g. `let mut`, `match `, `struct `).
  Comments don't count. Right output without the concept doesn't unlock the
  next lesson; you get a note saying what's missing.
* **CBEditor** (`edit file`, also `nano`/`vim`/`vi`) shows the lesson, the
  exercise, the hints you've revealed and the test results in a panel to
  the right of your code. `F5` saves and checks, `F2` reveals the next hint,
  `F1` toggles the panel, `^S` saves, `^X` exits. After a pass the panel
  switches straight to the next lesson.
* **Hints are graded.** Each `hint` (or `F2`) reveals one more nudge, never
  the solution.
* **Languages.** German is the default, English is available: `lang en` /
  `lang de`. The choice is saved in `settings.json`. Switching rewrites the
  lesson files but never your code. Expected outputs are translated too
  (`Summe: 6 Euro` vs. `Total: 6 Euro`), so `submit` checks against the
  current language.
* **Aliases** exist only for editors and English synonyms: `nano`, `vim`,
  `vi` → `edit`, `status` → `tasks`, `logout` → `exit`, `language` → `lang`.

## Writing a course

Add a `[[task]]` to `src-tauri/tasks/tasks.toml`. Each `[[task.part]]` is
one lesson:

```toml
[[task]]
id = "02_my_course"
title = { en = "My Course", de = "Mein Kurs" }
summary.en = "One line shown in the course list."
summary.de = "Eine Zeile für die Kursliste."
starter_file = "main.rs"
starter_code = '''
fn main() {}
'''

[[task.part]]
title = { en = "Variables", de = "Variablen" }
lesson.en = '''
Theory with a small example that is NOT the solution.
'''
lesson.de = '''
Theorie mit einem kleinen Beispiel, das NICHT die Lösung ist.
'''
description.en = '''
The exercise: what to add to the program, with expected output.
'''
description.de = '''
Die Aufgabe: was ins Programm kommt, mit erwarteter Ausgabe.
'''
hints = [{ en = "a nudge", de = "ein Denkanstoß" }]

[[task.part.require]]
code = "let "
message = { en = "Use let.", de = "Benutze let." }

[[task.part.test]]
input = "5\n"
expected = "25"
```

Any text field can be a plain string (same in every language) or
`{ en = "...", de = "..." }`. Put a reference solution for each lesson in
`tasks/solutions/<id>/partN.rs`. `cargo test` then runs the whole course in
every language. The catalog is compiled into the binary, so rebuild after
editing it.
