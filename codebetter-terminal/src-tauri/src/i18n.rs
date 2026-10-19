//! Languages, translatable catalog text and the persisted language setting.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    De,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::De, Lang::En];

    pub fn code(self) -> &'static str {
        match self {
            Lang::De => "de",
            Lang::En => "en",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::De => "Deutsch",
            Lang::En => "English",
        }
    }

    pub fn parse(s: &str) -> Option<Lang> {
        match s.to_lowercase().as_str() {
            "de" | "deutsch" | "german" => Some(Lang::De),
            "en" | "english" | "englisch" => Some(Lang::En),
            _ => None,
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
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Text {
    Same(String),
    Local { en: String, de: String },
}

impl Text {
    pub fn get(&self, lang: Lang) -> &str {
        match (self, lang) {
            (Text::Same(s), _) => s,
            (Text::Local { en, .. }, Lang::En) => en,
            (Text::Local { de, .. }, Lang::De) => de,
        }
    }
}

impl Default for Text {
    fn default() -> Self {
        Text::Same(String::new())
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Settings {
    #[serde(default)]
    pub lang: Lang,
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, json)
    }
}
