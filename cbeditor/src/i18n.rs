//! Languages and translatable catalog text, matching the CodeBetter terminal app.

use serde::Deserialize;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

impl Lang {
    pub fn parse(s: &str) -> Option<Lang> {
        match s.to_lowercase().as_str() {
            "de" | "deutsch" | "german" => Some(Lang::De),
            "en" | "english" | "englisch" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn pick<'a>(self, en: &'a str, de: &'a str) -> &'a str {
        match self {
            Lang::En => en,
            Lang::De => de,
        }
    }
}

/// `format!` with one template per language:
/// `tr!(lang, "{n} files", "{n} Dateien")`.
#[macro_export]
macro_rules! tr {
    ($lang:expr, $en:literal, $de:literal $(, $arg:expr)* $(,)?) => {
        match $lang {
            $crate::i18n::Lang::En => format!($en $(, $arg)*),
            $crate::i18n::Lang::De => format!($de $(, $arg)*),
        }
    };
}

/// Catalog text: either one string for every language, or `{ en = "..", de = ".." }`.
#[derive(Deserialize, Default)]
#[serde(untagged)]
pub enum Text {
    #[default]
    None,
    Same(String),
    Local {
        en: String,
        de: String,
    },
}

impl Text {
    pub fn get(&self, lang: Lang) -> &str {
        match self {
            Text::None => "",
            Text::Same(s) => s,
            Text::Local { en, de } => lang.pick(en, de),
        }
    }
}
