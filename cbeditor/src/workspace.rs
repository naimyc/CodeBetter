//! Course directories, like `~/Tasks.cb` in the CodeBetter terminal app:
//! every course gets `<root>/<id>/` with its starter file, a README and
//! the progress, so the next start goes on at the next lesson.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
use crate::lesson::{Course, Lesson};
use crate::tr;

const PROGRESS_FILE: &str = ".progress.toml";

#[derive(Serialize, Deserialize)]
struct Progress {
    course: String,
    /// Lessons passed.
    completed: usize,
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// `~/Tasks.cb`, or `Tasks.cb` in the current directory without a home.
pub fn default_root() -> PathBuf {
    home().unwrap_or_default().join("Tasks.cb")
}

/// `path` with the home directory written as `~`.
pub fn pretty(path: &Path) -> String {
    match home().and_then(|h| path.strip_prefix(h).ok().map(Path::to_owned)) {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => path.display().to_string(),
    }
}

/// Lessons of course `id` passed in `dir`; `None` if `dir` isn't that course.
pub fn completed(dir: &Path, id: &str) -> Option<usize> {
    let text = fs::read_to_string(dir.join(PROGRESS_FILE)).ok()?;
    let progress: Progress = toml::from_str(&text).ok()?;
    (progress.course == id).then_some(progress.completed)
}

/// Saves the progress of `lesson` and writes `partN.md` for every lesson
/// it unlocked.
pub fn save_progress(dir: &Path, lesson: &Lesson) -> io::Result<()> {
    save_completed(dir, lesson.course_id(), lesson.completed())?;
    for i in 0..lesson.unlocked() {
        fs::write(dir.join(format!("part{}.md", i + 1)), lesson.lesson_file(i))?;
    }
    Ok(())
}

pub fn save_completed(dir: &Path, id: &str, completed: usize) -> io::Result<()> {
    let progress = Progress {
        course: id.to_owned(),
        completed,
    };
    let text = toml::to_string(&progress).map_err(io::Error::other)?;
    fs::write(dir.join(PROGRESS_FILE), text)
}

/// Creates `<root>/<id>/` with the starter file (never overwritten, it's
/// the learner's code), the README and the progress. Returns the starter
/// file and whether the directory was new.
pub fn prepare(root: &Path, course: &Course, lang: Lang) -> io::Result<(PathBuf, bool)> {
    let dir = root.join(&course.id);
    let new = !dir.exists();
    fs::create_dir_all(&dir)?;
    let file = dir.join(&course.starter_file);
    if !file.exists() {
        fs::write(&file, &course.starter_code)?;
    }
    fs::write(dir.join("README.md"), readme(course, lang))?;
    if completed(&dir, &course.id).is_none() {
        save_completed(&dir, &course.id, 0)?;
    }
    Ok((file, new))
}

fn readme(course: &Course, lang: Lang) -> String {
    let heading = format!("{} · {}", course.id, course.title);
    let file = &course.starter_file;
    let n = course.lessons;
    let open = format!("cbeditor {file}");
    let body = tr!(
        lang,
        "{n} lessons. Every lesson extends the same program in {file}.\n\n\
         WORKFLOW\n\
         \x20 cbeditor            pick this course on the start screen\n\
         \x20 {open:<20}or open the file in this directory\n\
         \x20 cat part1.md        read a lesson (one file per unlocked lesson)\n\n\
         \x20 F5   check your solution; a pass unlocks the next lesson\n\
         \x20 F2   stuck? get a nudge, not the answer\n\n\
         Your progress is kept in {PROGRESS_FILE}.\n",
        "{n} Lektionen. Jede Lektion erweitert dasselbe Programm in {file}.\n\n\
         ABLAUF\n\
         \x20 cbeditor            diesen Kurs im Startbildschirm wählen\n\
         \x20 {open:<20}oder die Datei in diesem Ordner öffnen\n\
         \x20 cat part1.md        eine Lektion lesen (eine Datei pro freigeschaltete Lektion)\n\n\
         \x20 F5   Lösung prüfen; bestanden schaltet die nächste Lektion frei\n\
         \x20 F2   hängst du fest? Ein Denkanstoß, nicht die Lösung\n\n\
         Dein Fortschritt steht in {PROGRESS_FILE}.\n",
    );
    format!(
        "{heading}\n{}\n\n{}\n\n{body}",
        "=".repeat(heading.chars().count()),
        course.summary
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lesson;

    #[test]
    fn prepare_creates_the_course_and_keeps_the_code() {
        let root = std::env::temp_dir().join(format!("cbeditor-ws-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let course = lesson::courses(Lang::De)
            .into_iter()
            .find(|c| c.id == "02_java_basics")
            .unwrap();

        let (file, new) = prepare(&root, &course, Lang::De).unwrap();
        assert!(new);
        assert_eq!(file, root.join("02_java_basics/Main.java"));
        assert!(
            fs::read_to_string(&file)
                .unwrap()
                .contains("public class Main")
        );
        assert!(root.join("02_java_basics/README.md").exists());
        assert_eq!(
            completed(&root.join("02_java_basics"), "02_java_basics"),
            Some(0)
        );

        let lesson = Lesson::resume("02_java_basics", 1, Lang::De).unwrap();
        save_progress(file.parent().unwrap(), &lesson).unwrap();
        let part2 = fs::read_to_string(root.join("02_java_basics/part2.md")).unwrap();
        assert!(
            part2.starts_with("LEKTION 2/4 · Variablen und Typen"),
            "{part2}"
        );
        assert!(!root.join("02_java_basics/part3.md").exists());

        fs::write(&file, "my code").unwrap();
        save_completed(file.parent().unwrap(), "02_java_basics", 2).unwrap();
        let (_, new) = prepare(&root, &course, Lang::En).unwrap();
        assert!(!new);
        assert_eq!(fs::read_to_string(&file).unwrap(), "my code");
        assert_eq!(completed(file.parent().unwrap(), "02_java_basics"), Some(2));
        assert_eq!(completed(file.parent().unwrap(), "01_rust_basics"), None);
        let _ = fs::remove_dir_all(root);
    }
}
