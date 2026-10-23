//! Languages and translatable catalog text, matching the CodeBetter terminal app.

use serde::{Deserialize, Serialize, Serializer};

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
#[derive(Deserialize, Default, Clone)]
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

/// Written back the way it was read; a missing text becomes "".
impl Serialize for Text {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Local<'a> {
            en: &'a str,
            de: &'a str,
        }
        match self {
            Text::None => s.serialize_str(""),
            Text::Same(text) => s.serialize_str(text),
            Text::Local { en, de } => Local { en, de }.serialize(s),
        }
    }
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
