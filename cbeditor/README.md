# CBEditor (terminal)

The CBEditor of the CodeBetter terminal app as a native terminal program,
built with [crossterm](https://crates.io/crates/crossterm). It runs on Linux,
macOS and Windows (Windows Terminal, PowerShell, cmd).

```bash
cargo run                                              # start screen: pick a course
cargo run -- --task 02_java_basics                     # straight into a course
cargo run -- main.rs                                   # edit a file, Rust course
cargo run -- Main.java                                 # Java course (by file type)
cargo run -- main.rs --task 01_rust_basics --part 3 --lang en
```

## Start screen

Without a file, CBEditor starts like the app: the CB-BIOS boot lines (any
key skips them), the CodeBetter banner and the courses with their
language and progress:

```
  ══ SPRACHE & PROJEKT ════════════════════════════════════
  ▶ 1  Rust     Rust-Grundlagen: CB Shop     [###.......] 3/10
    2  Java     Java-Grundlagen: CB Kino     [..........] neu
    3  Python   Python-Grundlagen: CB Noten  [..........] neu
    4  C        C-Grundlagen: CB Thermo      [..........] neu

    +  Neue Aufgabe mit KI erstellen   KI:connect.nrw
    ⇩  Aufgabe importieren (.cbtask)
```

↑/↓ (or `1`–`9`) pick a course, Enter opens it, `E` exports it, `L`
switches between German and English, Esc quits. Opening a course sets up its directory in
`~/Tasks.cb` (`--dir DIR` for another place), like `~/Tasks.cb` in the app:

| File | Content |
|------|---------|
| `main.rs`, `Main.java`, `main.py`, `main.c` | the starter code; never overwritten, it's your code |
| `README.md` | what the course is and how to work on it |
| `part1.md`, `part2.md`, ... | one file per unlocked lesson (LERNEN, DEINE AUFGABE) |
| `.progress.toml` | how many lessons are passed |

The editor then opens the starter file at the next lesson. A pass with F5
saves the progress and writes the next `partN.md`. `^X` goes back to the
start screen, which shows the new progress.

Opening a file directly works too. Without `--task`, its course is picked by
the extension: `.rs` Rust, `.java` Java, `.py` Python, `.c` C (the first
course otherwise). If the file is in a course directory, the editor goes on
at the saved lesson (unless `--part` is given) and saves the progress there.

It looks like the app: green phosphor colours, an inverted top bar
(`▓▒░ CBEditor`, file name, `Geändert`), line numbers with a dashed rule, and
the lesson panel behind a double border on the right: LERNEN, DEINE AUFGABE
TIPPS and PRÜFUNG with yellow `══` headings. A centred status line and the key bar
sit at the bottom. German is the default; `--lang en` switches the editor and
the lesson to English.

The course text comes from `../codebetter-terminal/src-tauri/tasks/tasks.toml`
and is compiled into the binary, so rebuild after editing it. The panel is
hidden when the terminal is narrower than 60 columns.

| Key | Action |
|-----|--------|
| `^S` | save |
| `^Z` / `^Y` | undo / redo (a typed word is one step) |
| `^K` / `^U` | cut the line (again: add the next one) / paste the cut lines above |
| F5 | save and check: compile, run the lesson's tests, unlock the next lesson on a pass |
| `^X` (or `^Q`) | exit, back to the start screen if you came from there; with unsaved changes asks: `J`/`Y` save, `N` discard, `Esc` cancel |
| F2 | reveal the next hint (it flashes) |
| F1 | show/hide the lesson panel |
| Tab / Shift+Tab | indent to the next 4-space stop / outdent by 4 spaces |
| Enter | new line, keeping the indentation (one level more after `{ ( [`, and after `:` in Python); between `{}` the `}` goes on its own line |
| Backspace | in the indentation removes a whole level; between `()` removes both |
| Home | to the first non-space, again to column 0 |
| Ctrl+←/→, Ctrl+Home/End | by word, to the start/end of the file |
| mouse wheel | scrolls the panel when over it, otherwise moves through the code |
| Alt+↑/↓ or Ctrl+↑/↓ | scroll the lesson panel by a line |
| Alt+PgUp / Alt+PgDn | scroll the lesson panel by a page |
| arrows, Home, End, PgUp, PgDn | move |

## Editing

Code is highlighted by file type (`.rs`, `.c`/`.h`, `.java`, `.py`,
`.js`): keywords, types, strings, numbers, comments and macros
(`println!`, `#include`, `@Override`). Block comments and multi-line strings
carry over to the next lines. The cursor line is slightly lighter, and the
bracket at the cursor lights up together with its partner, or red when it
has none.

Typing `(`, `[`, `{` or `"` adds the closing one; typing it again steps
over it. `'` is closed too, except in Rust (lifetimes) and after a letter
(`don't`). A `}` at the start of a line moves back one level.

Pasted text goes in as it is, without auto-indent or closing brackets, and
is one undo step: via bracketed paste where the terminal supports it, and
on Windows by noticing that the keys arrive faster than anyone types.

## Own courses: AI and `.cbtask`

The two rows under the courses add more of them. Own courses are kept in
`~/Tasks.cb/.custom/<id>.cbtask` and listed with "(eigene)".

**Create a course with AI.** Enter on `+` asks for a topic, e.g.
`Python: Listen und Schleifen`. While the AI works, a pane shows what is
going on, like a chat UI: a spinner, the time, how much has come in,
whether the model is still thinking or which lesson it is writing, and
the last line or two of its answer as it streams in. Esc cancels. The AI writes a whole course in the
format of `tasks.toml`, with a reference solution per lesson. Before
the course is kept, each reference solution has to pass its lesson
(tests and code checks). If one fails, the report goes back to the AI
once to be fixed. The solutions are then dropped, so the saved course
doesn't give them away. The reference solutions run on your computer
like your own code, with the same 5 second limit per test.

Any OpenAI-compatible chat API works; the default is
[KI:connect.nrw](https://www.itc.rwth-aachen.de/cms/it-center/services/kollaboration/~bndnjc/ki-connect/), the AI
service of the universities in NRW (`https://chat.kiconnect.nrw/api/v1`).
Create an API key there (log in, your name at the bottom left, API Key
Management). The easiest way to store it: `S` on the `+` row, paste the
key, Enter. It goes into `~/Tasks.cb/.ki.toml`, which only you can read.
You can also write the file yourself; the key alone is enough, even with
`=`, `/` or `:` in it, and so is `KEY=...` as in a `.env` file:

```
sk-...
```

For more settings (another API, a fixed model) write it as TOML instead:

```toml
provider = "kiconnect"   # default; "openai", or any name with base_url
api_key = "..."
# model = "..."          # optional, see below
# base_url = "https://chat.kiconnect.nrw/api/v1"
```

With KI:connect.nrw the course is written by one of the models for
students, by default the first of these; `M` on the `+` row switches to
the next one and saves it in `.ki.toml` (the key stays):

| Model | id |
|-------|----|
| Qwen 3.5 397B | `qwen3.5-397b-a17b_stud` |
| GLM 4.7 | `glm-4.7_stud` |
| Qwen 3.8 | `qwen3.8_stud` |
| Mistral Small 4 | `mistralai-mistral-small-4_STUD` |
| Gemma 3 27B | `gemma-3-27b-it_stud` |

Another id works with `model = "..."`. Other APIs without a `model` use
the first one their `/models` endpoint lists.

Environment variables override the file: `CB_AI_PROVIDER`,
`CB_AI_BASE_URL`, `CB_AI_MODEL`, and `CB_AI_KEY` (or `KICONNECT_API_KEY`).
The start screen says what is missing. Keep the file to yourself: it holds
your key.

**Import and export.** `E` on a course writes it to a `.cbtask` file
(`<id>.cbtask` in the current directory unless you type another path);
Enter on `⇩` asks for a `.cbtask` file and adds that course. A `.cbtask`
file is one `[task]` of `tasks.toml` with a format version:

```toml
format = 1

[task]
id = "mein_kurs"            # a-z, 0-9 and _
title = "Mein Kurs"
starter_file = "main.py"    # .rs .c .java .py or .js
...
[[task.part]]
...
```

Importing checks the format but runs nothing. A course with the id of a
built-in one is refused; one with the id of an own course replaces it.

## Checking (F5)

F5 works like `submit` in the app. It saves, compiles (`rustc`, `cc` for
`.c`, `javac` for `.java`; `python3` and `node` run `.py` and `.js`), feeds every test's input to
the program and compares the output (trailing whitespace doesn't count).
PRÜFUNG shows `ok` or `FEHLGESCHLAGEN` per test. For the first failure it
shows the input, the expected output and what came out; hidden tests don't
show their input. If the output is right but the code doesn't use the
lesson's concept yet (e.g. `let mut`), it says what's missing. That check
compares tokens, so spacing doesn't matter (`int *` matches `int*p` and
`int * p`), words match whole (`for` isn't in `format`), and comments,
`/* */` included, don't count. A pass
unlocks the next lesson right away, and its title flashes. Each test has a
5 second time limit.

Progress is saved in course directories only (see above); a file elsewhere
starts at `--part` every time.

The editor captures the mouse for the wheel, so to select text with the
mouse hold Shift while dragging.

Colours need a terminal with 24-bit colour (almost all current ones,
including Windows Terminal).

| Part | File |
|------|------|
| Editing, keys, arguments | `src/main.rs` |
| Start screen: boot, banner, course list | `src/menu.rs` |
| Course directories and progress | `src/workspace.rs` |
| Screen layout, bracket matching | `src/draw.rs` |
| Syntax highlighting | `src/highlight.rs` |
| Colours (from `ui/style.css`) | `src/theme.rs` |
| Language, `tr!` macro | `src/i18n.rs` |
| Course catalog: built-in and own courses, `.cbtask` import/export | `src/catalog.rs` |
| Panel content, unlocking | `src/lesson.rs` |
| Creating courses with AI (KI:connect.nrw or another API) | `src/ai.rs` |
| F5: tests and concept checks | `src/check.rs` |
| Compile and run with a time limit | `src/runner.rs` |

`cargo test` checks that every lesson renders in both languages and that
each reference solution in `tasks/solutions/` passes its lesson in both
languages (needs `rustc`, `javac`/`java`, `python3` and `cc`).
