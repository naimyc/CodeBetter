# CBEditor (terminal)

The CBEditor of the CodeBetter terminal app as a native terminal program,
built with [crossterm](https://crates.io/crates/crossterm). It runs on Linux,
macOS and Windows (Windows Terminal, PowerShell, cmd).

```bash
cargo run -- main.rs                                   # Rust course, lesson 1
cargo run -- Main.java                                 # Java course (by file type)
cargo run -- main.rs --task 01_rust_basics --part 3 --lang en
```

Without `--task`, the course is picked by the file's extension: `.rs` Rust,
`.java` Java, `.py` Python, `.c` C (the first course otherwise).

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
| `^X` (or `^Q`) | exit; with unsaved changes asks: `J`/`Y` save, `N` discard, `Esc` cancel |
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

## Checking (F5)

F5 works like `submit` in the app. It saves, compiles (`rustc`, `cc` for
`.c`, `javac` for `.java`; `python3` and `node` run `.py` and `.js`), feeds every test's input to
the program and compares the output (trailing whitespace doesn't count).
PRÜFUNG shows `ok` or `FEHLGESCHLAGEN` per test. For the first failure it
shows the input, the expected output and what came out; hidden tests don't
show their input. If the output is right but the code doesn't use the
lesson's concept yet (e.g. `let mut`), it says what's missing. A pass
unlocks the next lesson right away, and its title flashes. Each test has a
5 second time limit.

Progress isn't saved yet: the next start begins at `--part` again.

The editor captures the mouse for the wheel, so to select text with the
mouse hold Shift while dragging.

Colours need a terminal with 24-bit colour (almost all current ones,
including Windows Terminal).

| Part | File |
|------|------|
| Editing, keys, arguments | `src/main.rs` |
| Screen layout, bracket matching | `src/draw.rs` |
| Syntax highlighting | `src/highlight.rs` |
| Colours (from `ui/style.css`) | `src/theme.rs` |
| Language, `tr!` macro | `src/i18n.rs` |
| Course catalog, panel content, unlocking | `src/lesson.rs` |
| F5: tests and concept checks | `src/check.rs` |
| Compile and run with a time limit | `src/runner.rs` |

`cargo test` checks that every lesson renders in both languages and that
each reference solution in `tasks/solutions/` passes its lesson in both
languages (needs `rustc`, `javac`/`java`, `python3` and `cc`).
