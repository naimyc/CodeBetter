//! Syntax highlighting for the code area. Each line is split into kinds
//! (keyword, string, comment, ...) one char at a time; a `State` carries
//! block comments and multi-line strings over to the next line.

use std::path::Path;

/// What a char of code is, which decides its color.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Text,
    Keyword,
    Type,
    Str,
    Number,
    Comment,
    /// Rust macros like `println!`, C's `#include`, `@Override`.
    Macro,
}

/// What is still open at the end of a line.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum State {
    #[default]
    Code,
    Comment,
    Str {
        quote: char,
        triple: bool,
    },
}

/// The rules of one language.
pub struct Syntax {
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    line_comment: &'static str,
    block_comment: Option<(&'static str, &'static str)>,
    quotes: &'static [char],
    /// Quotes whose strings may go on over the line end.
    multiline: &'static [char],
    /// Python's `"""` and `'''`.
    triple_quotes: bool,
    /// Rust: `'a` is a lifetime unless it closes like `'a'`.
    lifetimes: bool,
    /// Rust: `name!` is a macro.
    bang_macros: bool,
    /// C: a line starting with `#` is a preprocessor line.
    preprocessor: bool,
    /// Java and Python: `@name` annotations and decorators.
    annotations: bool,
    /// Rust and Java: `Capitalized` names are types.
    capitalized_types: bool,
    /// Rust and Python: string prefixes like `b"` or `f"` belong to the string.
    string_prefixes: &'static [&'static str],
    /// Python: `:` at a line end opens a block.
    pub colon_blocks: bool,
    /// Whether `'` gets closed automatically when typed.
    pub close_single_quote: bool,
}

const RUST: Syntax = Syntax {
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while",
    ],
    types: &[
        "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str", "u8",
        "u16", "u32", "u64", "u128", "usize",
    ],
    line_comment: "//",
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\''],
    multiline: &['"'],
    triple_quotes: false,
    lifetimes: true,
    bang_macros: true,
    preprocessor: false,
    annotations: false,
    capitalized_types: true,
    string_prefixes: &["b", "r", "br"],
    colon_blocks: false,
    close_single_quote: false,
};

const C: Syntax = Syntax {
    keywords: &[
        "break", "case", "const", "continue", "default", "do", "else", "enum", "extern", "for",
        "goto", "if", "inline", "register", "return", "sizeof", "static", "struct", "switch",
        "typedef", "union", "volatile", "while", "NULL", "true", "false",
    ],
    types: &[
        "bool", "char", "double", "float", "int", "long", "short", "signed", "unsigned", "void",
        "size_t", "FILE", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t",
        "uint32_t", "uint64_t",
    ],
    line_comment: "//",
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\''],
    multiline: &[],
    triple_quotes: false,
    lifetimes: false,
    bang_macros: false,
    preprocessor: true,
    annotations: false,
    capitalized_types: false,
    string_prefixes: &[],
    colon_blocks: false,
    close_single_quote: true,
};

const JAVA: Syntax = Syntax {
    keywords: &[
        "abstract",
        "assert",
        "break",
        "case",
        "catch",
        "class",
        "continue",
        "default",
        "do",
        "else",
        "enum",
        "extends",
        "final",
        "finally",
        "for",
        "if",
        "implements",
        "import",
        "instanceof",
        "interface",
        "native",
        "new",
        "package",
        "private",
        "protected",
        "public",
        "record",
        "return",
        "static",
        "super",
        "switch",
        "synchronized",
        "this",
        "throw",
        "throws",
        "try",
        "var",
        "void",
        "volatile",
        "while",
        "true",
        "false",
        "null",
    ],
    types: &[
        "boolean", "byte", "char", "double", "float", "int", "long", "short",
    ],
    line_comment: "//",
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\''],
    multiline: &[],
    triple_quotes: false,
    lifetimes: false,
    bang_macros: false,
    preprocessor: false,
    annotations: true,
    capitalized_types: true,
    string_prefixes: &[],
    colon_blocks: false,
    close_single_quote: true,
};

const PYTHON: Syntax = Syntax {
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is",
        "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with",
        "yield", "True", "False", "None", "self",
    ],
    types: &[
        "bool", "bytes", "dict", "float", "int", "list", "object", "set", "str", "tuple", "print",
        "input", "len", "range",
    ],
    line_comment: "#",
    block_comment: None,
    quotes: &['"', '\''],
    multiline: &[],
    triple_quotes: true,
    lifetimes: false,
    bang_macros: false,
    preprocessor: false,
    annotations: true,
    capitalized_types: false,
    string_prefixes: &["f", "r", "b", "u", "rb", "br", "fr", "rf"],
    colon_blocks: true,
    close_single_quote: true,
};

const JAVASCRIPT: Syntax = Syntax {
    keywords: &[
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "default",
        "delete",
        "do",
        "else",
        "export",
        "extends",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "let",
        "new",
        "of",
        "return",
        "static",
        "super",
        "switch",
        "this",
        "throw",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "yield",
        "true",
        "false",
        "null",
        "undefined",
    ],
    types: &[],
    line_comment: "//",
    block_comment: Some(("/*", "*/")),
    quotes: &['"', '\'', '`'],
    multiline: &['`'],
    triple_quotes: false,
    lifetimes: false,
    bang_macros: false,
    preprocessor: false,
    annotations: false,
    capitalized_types: true,
    string_prefixes: &[],
    colon_blocks: false,
    close_single_quote: true,
};

/// Plain text: nothing is highlighted.
const PLAIN: Syntax = Syntax {
    keywords: &[],
    types: &[],
    line_comment: "",
    block_comment: None,
    quotes: &[],
    multiline: &[],
    triple_quotes: false,
    lifetimes: false,
    bang_macros: false,
    preprocessor: false,
    annotations: false,
    capitalized_types: false,
    string_prefixes: &[],
    colon_blocks: false,
    close_single_quote: false,
};

impl Syntax {
    /// The language of a file, by its extension.
    pub fn for_path(path: &str) -> &'static Syntax {
        match Path::new(path).extension().and_then(|e| e.to_str()) {
            Some("rs") => &RUST,
            Some("c" | "h") => &C,
            Some("java") => &JAVA,
            Some("py") => &PYTHON,
            Some("js" | "mjs") => &JAVASCRIPT,
            _ => &PLAIN,
        }
    }

    /// The kinds of every line of a file.
    pub fn highlight_all(&self, lines: &[String]) -> Vec<Vec<Kind>> {
        let mut state = State::Code;
        lines
            .iter()
            .map(|line| {
                let (kinds, next) = self.highlight(line, state);
                state = next;
                kinds
            })
            .collect()
    }

    /// The kind of each char of `line`, and what is still open after it.
    pub fn highlight(&self, line: &str, mut state: State) -> (Vec<Kind>, State) {
        let chars: Vec<char> = line.chars().collect();
        let mut kinds = vec![Kind::Text; chars.len()];
        let mut i = 0;
        while i < chars.len() {
            match state {
                State::Comment => {
                    let end = self.block_comment.map_or("", |(_, end)| end);
                    if starts_with(&chars, i, end) {
                        let n = end.chars().count();
                        kinds[i..i + n].fill(Kind::Comment);
                        i += n;
                        state = State::Code;
                    } else {
                        kinds[i] = Kind::Comment;
                        i += 1;
                    }
                    continue;
                }
                State::Str { quote, triple } => {
                    let n = if triple { 3 } else { 1 };
                    if chars[i] == '\\' {
                        let n = 2.min(chars.len() - i);
                        kinds[i..i + n].fill(Kind::Str);
                        i += n;
                    } else if chars[i..].iter().take(n).filter(|c| **c == quote).count() == n {
                        kinds[i..i + n].fill(Kind::Str);
                        i += n;
                        state = State::Code;
                    } else {
                        kinds[i] = Kind::Str;
                        i += 1;
                    }
                    continue;
                }
                State::Code => {}
            }

            let c = chars[i];
            if !self.line_comment.is_empty() && starts_with(&chars, i, self.line_comment) {
                kinds[i..].fill(Kind::Comment);
                break;
            }
            if let Some((start, _)) = self.block_comment
                && starts_with(&chars, i, start)
            {
                let n = start.chars().count();
                kinds[i..i + n].fill(Kind::Comment);
                i += n;
                state = State::Comment;
                continue;
            }
            if self.preprocessor && c == '#' && chars[..i].iter().all(|c| c.is_whitespace()) {
                // `#include <stdio.h>`: the directive, then the rest as a string.
                let word_end = i + 1 + ident_len(&chars[i + 1..]);
                kinds[i..word_end].fill(Kind::Macro);
                let rest: String = chars[word_end..].iter().collect();
                if rest.trim_start().starts_with(['<', '"']) {
                    kinds[word_end..].fill(Kind::Str);
                }
                break;
            }
            if self.quotes.contains(&c) {
                if self.lifetimes && c == '\'' && !is_char_literal(&chars[i..]) {
                    // A lifetime like 'a or a label like 'outer.
                    let n = 1 + ident_len(&chars[i + 1..]);
                    kinds[i..i + n].fill(Kind::Type);
                    i += n;
                    continue;
                }
                let triple = self.triple_quotes
                    && chars[i..].iter().take(3).filter(|q| **q == c).count() == 3;
                let n = if triple { 3 } else { 1 };
                kinds[i..i + n].fill(Kind::Str);
                i += n;
                state = State::Str { quote: c, triple };
                continue;
            }
            if c.is_ascii_digit() {
                let n = chars[i..]
                    .iter()
                    .take_while(|c| c.is_alphanumeric() || **c == '_' || **c == '.')
                    .count();
                kinds[i..i + n].fill(Kind::Number);
                i += n;
                continue;
            }
            if c == '@' && self.annotations {
                let n = 1 + chars[i + 1..]
                    .iter()
                    .take_while(|c| is_ident(**c) || **c == '.')
                    .count();
                kinds[i..i + n].fill(Kind::Macro);
                i += n;
                continue;
            }
            if is_ident_start(c) {
                let n = ident_len(&chars[i..]);
                let word: String = chars[i..i + n].iter().collect();
                let next = chars.get(i + n).copied();
                let kind = if next.is_some_and(|q| self.quotes.contains(&q))
                    && self.string_prefixes.contains(&word.as_str())
                {
                    Kind::Str
                } else if self.bang_macros
                    && next == Some('!')
                    && chars.get(i + n + 1) != Some(&'=')
                {
                    kinds[i + n] = Kind::Macro;
                    Kind::Macro
                } else if self.keywords.contains(&word.as_str()) {
                    Kind::Keyword
                } else if self.types.contains(&word.as_str())
                    || (self.capitalized_types && c.is_ascii_uppercase())
                {
                    Kind::Type
                } else {
                    Kind::Text
                };
                kinds[i..i + n].fill(kind);
                i += n;
                continue;
            }
            i += 1;
        }
        // Most strings end with their line, even when they aren't closed.
        if let State::Str { quote, triple } = state
            && !triple
            && !self.multiline.contains(&quote)
        {
            state = State::Code;
        }
        (kinds, state)
    }
}

fn starts_with(chars: &[char], at: usize, pat: &str) -> bool {
    (at..).zip(pat.chars()).all(|(i, p)| chars.get(i) == Some(&p))
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn ident_len(chars: &[char]) -> usize {
    chars.iter().take_while(|c| is_ident(**c)).count()
}

/// Whether `'...` (starting at the quote) is a char literal like `'a'` or `'\n'`.
fn is_char_literal(chars: &[char]) -> bool {
    match chars.get(1) {
        Some('\\') => true,
        Some(_) => chars.get(2) == Some(&'\''),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Kind::*;

    /// The kinds as one letter per char: t text, k keyword, y type, s string,
    /// n number, c comment, m macro.
    fn code(syntax: &Syntax, line: &str, state: State) -> (String, State) {
        let (kinds, state) = syntax.highlight(line, state);
        let letters = kinds
            .iter()
            .map(|k| match k {
                Text => 't',
                Keyword => 'k',
                Type => 'y',
                Str => 's',
                Number => 'n',
                Comment => 'c',
                Macro => 'm',
            })
            .collect();
        (letters, state)
    }

    #[test]
    fn rust_keywords_strings_macros_and_comments() {
        let (k, s) = code(
            &RUST,
            r#"let x: u32 = 5; println!("a\"b"); // hi"#,
            State::Code,
        );
        assert_eq!(k, "kkkttttyyytttnttmmmmmmmmtsssssstttccccc");
        assert_eq!(s, State::Code);
        let (k, _) = code(&RUST, "fn f<'a>(c: char) -> char { 'x' }", State::Code);
        assert_eq!(k, "kktttyytttttyyyytttttyyyytttssstt");
    }

    #[test]
    fn block_comments_and_strings_go_on_over_lines() {
        let (k, s) = code(&C, "int a; /* start", State::Code);
        assert_eq!(k, "yyyttttcccccccc");
        assert_eq!(s, State::Comment);
        let (k, s) = code(&C, "end */ x", s);
        assert_eq!(k, "cccccctt");
        assert_eq!(s, State::Code);

        let (_, s) = code(&PYTHON, r#"text = """first"#, State::Code);
        assert_eq!(
            s,
            State::Str {
                quote: '"',
                triple: true
            }
        );
        let (k, s) = code(&PYTHON, r#"last""" # done"#, s);
        assert_eq!(k, "ssssssstcccccc");
        assert_eq!(s, State::Code);
    }

    #[test]
    fn c_preprocessor_and_java_annotations() {
        let (k, _) = code(&C, "#include <stdio.h>", State::Code);
        assert_eq!(k, "mmmmmmmmssssssssss");
        let (k, _) = code(&JAVA, "@Override String s;", State::Code);
        assert_eq!(k, "mmmmmmmmmtyyyyyyttt");
    }

    #[test]
    fn python_f_strings_and_floor_division() {
        let (k, _) = code(&PYTHON, r#"print(f"{a}") // 2"#, State::Code);
        assert_eq!(k, "yyyyytsssssstttttn");
    }

    #[test]
    fn unclosed_string_ends_with_the_line_in_c() {
        let (_, s) = code(&C, r#"printf("oops"#, State::Code);
        assert_eq!(s, State::Code);
    }
}
