//! The course catalog: the built-in courses (tasks.toml, shared with the
//! CodeBetter terminal app) and the learner's own ones, generated with AI
//! or imported from a `.cbtask` file. Own courses live in
//! `<root>/.custom/<id>.cbtask`.
//!
//! A `.cbtask` file is one course in the format of tasks.toml:
//!
//! ```toml
//! format = 1
//!
//! [task]
//! id = "ki_listen"
//! title = "Python: Listen"
//! starter_file = "main.py"
//! ...
//! [[task.part]]
//! ...
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::check::{Requirement, TestCase};
use crate::i18n::{Lang, Text};
use crate::tr;

/// Same catalog the Tauri app uses, compiled in.
const BUILT_IN: &str = include_str!("../../codebetter-terminal/src-tauri/tasks/tasks.toml");
/// The file extension of an exported course.
pub const EXTENSION: &str = "cbtask";
const FORMAT: u32 = 1;
/// Files a course can be written in: what the runner can build.
const LANGUAGES: [&str; 5] = ["rs", "c", "java", "py", "js"];

static CUSTOM_DIR: OnceLock<PathBuf> = OnceLock::new();

#[derive(Deserialize)]
struct Catalog {
    #[serde(rename = "task")]
    tasks: Vec<Task>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub title: Text,
    #[serde(default)]
    pub summary: Text,
    /// The file the course is written in, like `main.rs`.
    pub starter_file: String,
    #[serde(default)]
    pub starter_code: Text,
    #[serde(rename = "part")]
    pub parts: Vec<Part>,
    /// Generated or imported, not built in.
    #[serde(skip)]
    pub custom: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Part {
    pub title: Text,
    #[serde(default)]
    pub lesson: Text,
    pub description: Text,
    #[serde(default)]
    pub hints: Vec<Text>,
    #[serde(default, rename = "require", skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<Requirement>,
    #[serde(rename = "test")]
    pub tests: Vec<TestCase>,
    /// A reference solution. Generated courses bring one per lesson so
    /// they can be checked; it is dropped before the course is saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution: Option<String>,
}

/// A `.cbtask` file.
#[derive(Serialize, Deserialize)]
struct TaskFile {
    format: u32,
    task: Task,
}

/// Where own courses are kept; set once at the start.
pub fn use_custom_dir(dir: PathBuf) {
    let _ = CUSTOM_DIR.set(dir);
}

fn custom_dir() -> Option<&'static Path> {
    CUSTOM_DIR.get().map(PathBuf::as_path)
}

pub fn built_in() -> Vec<Task> {
    toml::from_str::<Catalog>(BUILT_IN)
        .expect("tasks.toml is checked by the tests")
        .tasks
}

/// The built-in courses, then the own ones by id. Own files that don't
/// read are left out.
pub fn all() -> Vec<Task> {
    let mut tasks = built_in();
    let Some(dir) = custom_dir() else {
        return tasks;
    };
    let mut own: Vec<Task> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == EXTENSION))
        .filter_map(|p| parse(&fs::read_to_string(p).ok()?, Lang::En).ok())
        .collect();
    own.sort_by(|a, b| a.id.cmp(&b.id));
    tasks.extend(own.into_iter().map(|t| Task { custom: true, ..t }));
    tasks
}

pub fn find(id: &str) -> Option<Task> {
    all().into_iter().find(|t| t.id == id)
}

/// Reads a `.cbtask` file (a bare `[task]` table is fine too) and checks
/// that it is a course the editor can run.
pub fn parse(text: &str, lang: Lang) -> Result<Task, String> {
    #[derive(Deserialize)]
    struct Loose {
        #[serde(default)]
        format: Option<u32>,
        task: Task,
    }
    let file: Loose = toml::from_str(text).map_err(|e| {
        tr!(
            lang,
            "not a CodeBetter task: {e}",
            "keine CodeBetter-Aufgabe: {e}"
        )
    })?;
    if file.format.is_some_and(|f| f > FORMAT) {
        return Err(lang
            .pick(
                "made by a newer CBEditor, please update",
                "mit einem neueren CBEditor erstellt, bitte aktualisieren",
            )
            .into());
    }
    validate(&file.task, lang)?;
    Ok(file.task)
}

fn validate(task: &Task, lang: Lang) -> Result<(), String> {
    let bad = |what: String| Err(what);
    if task.id.is_empty()
        || !task
            .id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return bad(tr!(
            lang,
            "id {:?} may only use a-z, 0-9 and _",
            "id {:?} darf nur a-z, 0-9 und _ enthalten",
            task.id
        ));
    }
    let file = Path::new(&task.starter_file);
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
    if file.components().count() != 1 || !LANGUAGES.contains(&ext) {
        return bad(tr!(
            lang,
            "starter_file {:?} must be a plain file name ending in .rs, .c, .java, .py or .js",
            "starter_file {:?} muss ein einfacher Dateiname mit .rs, .c, .java, .py oder .js sein",
            task.starter_file
        ));
    }
    if task.parts.is_empty() {
        return bad(lang
            .pick("the course has no lessons", "der Kurs hat keine Lektionen")
            .into());
    }
    for (i, part) in task.parts.iter().enumerate() {
        if part.tests.is_empty() {
            return bad(tr!(
                lang,
                "lesson {} has no tests",
                "Lektion {} hat keine Tests",
                i + 1
            ));
        }
    }
    Ok(())
}

/// The course as a `.cbtask` file, without reference solutions.
pub fn to_file(task: &Task) -> Result<String, String> {
    let mut task = task.clone();
    for part in &mut task.parts {
        part.solution = None;
    }
    let body = toml::to_string_pretty(&TaskFile {
        format: FORMAT,
        task,
    })
    .map_err(|e| e.to_string())?;
    Ok(format!(
        "# CodeBetter course. Import it on the CBEditor start screen.\n\n{body}"
    ))
}

/// Saves an own course in the custom directory. Returns whether it
/// replaced an own course with the same id.
pub fn save_custom(task: &Task, lang: Lang) -> Result<bool, String> {
    if built_in().iter().any(|t| t.id == task.id) {
        return Err(tr!(
            lang,
            "{} is a built-in course",
            "{} ist ein eingebauter Kurs",
            task.id
        ));
    }
    let dir = custom_dir().ok_or("no directory for own courses")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.{EXTENSION}", task.id));
    let replaced = path.exists();
    fs::write(&path, to_file(task)?).map_err(|e| e.to_string())?;
    Ok(replaced)
}

/// Imports the course in the `.cbtask` file at `path`.
pub fn import(path: &Path, lang: Lang) -> Result<(Task, bool), String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let task = parse(&text, lang)?;
    let replaced = save_custom(&task, lang)?;
    Ok((task, replaced))
}

/// Writes course `id` to `path` as a `.cbtask` file.
pub fn export(id: &str, path: &Path) -> Result<(), String> {
    let task = find(id).ok_or_else(|| format!("no course {id}"))?;
    fs::write(path, to_file(&task)?).map_err(|e| format!("{}: {e}", path.display()))
}

/// A free id like `ki_python_listen` for a course called `title`.
pub fn new_id(prefix: &str, title: &str) -> String {
    let mut slug = String::new();
    for c in title.to_lowercase().chars() {
        let c = match c {
            'ä' => "ae".into(),
            'ö' => "oe".into(),
            'ü' => "ue".into(),
            'ß' => "ss".into(),
            c if c.is_ascii_alphanumeric() => c.to_string(),
            _ => "_".into(),
        };
        if !(c == "_" && (slug.is_empty() || slug.ends_with('_'))) {
            slug.push_str(&c);
        }
    }
    let base: String = format!("{prefix}_{}", slug.trim_end_matches('_'))
        .chars()
        .take(40)
        .collect();
    let taken: Vec<String> = all().into_iter().map(|t| t.id).collect();
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}_{n}");
        n += 1;
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_course_survives_export_and_import() {
        for task in built_in() {
            let file = to_file(&task).unwrap();
            let back = parse(&file, Lang::De).unwrap_or_else(|e| panic!("{}: {e}", task.id));
            assert_eq!(back.id, task.id);
            assert_eq!(back.parts.len(), task.parts.len());
            for (a, b) in task.parts.iter().zip(&back.parts) {
                assert_eq!(a.description.get(Lang::En), b.description.get(Lang::En));
                assert_eq!(a.description.get(Lang::De), b.description.get(Lang::De));
                assert_eq!(a.tests.len(), b.tests.len());
                for (x, y) in a.tests.iter().zip(&b.tests) {
                    assert_eq!(x.expected.get(Lang::De), y.expected.get(Lang::De));
                    assert_eq!(x.input.get(Lang::En), y.input.get(Lang::En));
                    assert_eq!(x.hidden, y.hidden);
                }
            }
        }
    }

    #[test]
    fn broken_files_are_explained() {
        assert!(
            parse("no toml here", Lang::De)
                .err()
                .unwrap()
                .contains("keine CodeBetter-Aufgabe")
        );
        let task = to_file(&built_in()[0]).unwrap();
        let evil = task.replace("starter_file = \"main.rs\"", "starter_file = \"../x.rs\"");
        assert!(
            parse(&evil, Lang::En)
                .err()
                .unwrap()
                .contains("plain file name")
        );
        let newer = task.replace("format = 1", "format = 99");
        assert!(parse(&newer, Lang::En).err().unwrap().contains("newer"));
    }

    #[test]
    fn ids_are_slugs() {
        assert_eq!(
            new_id("ki", "Python: Listen & Schleifen!"),
            "ki_python_listen_schleifen"
        );
        assert_eq!(new_id("ki", "Größen"), "ki_groessen");
    }
}
