//! Compiles and runs a learner's source file with a time limit. Ported from
//! the CodeBetter terminal app; finds tools on Windows too.

use std::collections::hash_map::DefaultHasher;
use std::env::consts::EXE_SUFFIX;
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::i18n::Lang;
use crate::tr;

const MAX_OUTPUT: u64 = 64 * 1024;
const SUPPORTED: &str = ".rs .py .js .c .java";

pub struct Program {
    cmd: PathBuf,
    args: Vec<OsString>,
    dir: PathBuf,
    /// Compiler warnings, if any.
    pub compile_log: String,
}

pub struct RunResult {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub timed_out: bool,
}

/// Looks in PATH and in ~/.cargo/bin, which isn't always on PATH.
fn find_tool(names: &[&str]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        dirs.push(Path::new(&home).join(".cargo").join("bin"));
    }
    names
        .iter()
        .flat_map(|name| {
            dirs.iter()
                .map(move |d| d.join(format!("{name}{EXE_SUFFIX}")))
        })
        .find(|p| p.is_file())
}

fn not_found(tool: &str, lang: Lang) -> String {
    tr!(
        lang,
        "{tool} not found, is it installed?",
        "{tool} nicht gefunden. Ist es installiert?"
    )
}

fn compile(tool: &[&str], args: Vec<OsString>, dir: &Path, lang: Lang) -> Result<String, String> {
    let compiler = find_tool(tool).ok_or_else(|| not_found(tool[0], lang))?;
    let out = Command::new(compiler)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| {
            tr!(
                lang,
                "could not start {}: {e}",
                "konnte {} nicht starten: {e}",
                tool[0]
            )
        })?;
    let log = String::from_utf8_lossy(&out.stderr).into_owned();
    if out.status.success() {
        Ok(log)
    } else {
        Err(log)
    }
}

/// Turns a source file into something runnable. Compile errors come back as `Err`.
pub fn prepare(source: &Path, lang: Lang) -> Result<Program, String> {
    let source = std::path::absolute(source).map_err(|e| e.to_string())?;
    let dir = source.parent().unwrap_or(Path::new("/")).to_path_buf();
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("");
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("prog");
    let build_dir = std::env::temp_dir().join("codebetter-build");
    std::fs::create_dir_all(&build_dir).map_err(|e| e.to_string())?;
    // One binary per source file, so two tasks' main.rs never overwrite each other.
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let hash = hasher.finish();
    let binary = build_dir.join(format!("{stem}-{hash:x}{EXE_SUFFIX}"));

    let interpreted = |names: &[&str]| -> Result<Program, String> {
        let cmd = find_tool(names).ok_or_else(|| not_found(names[0], lang))?;
        Ok(Program {
            cmd,
            args: vec![source.clone().into()],
            dir: dir.clone(),
            compile_log: String::new(),
        })
    };

    match ext {
        "rs" => {
            let args = vec![
                "--edition".into(),
                "2021".into(),
                "--color".into(),
                "never".into(),
                "-C".into(),
                "debuginfo=0".into(),
                "-o".into(),
                binary.clone().into(),
                source.clone().into(),
            ];
            let compile_log = compile(&["rustc"], args, &dir, lang)?;
            Ok(Program {
                cmd: binary,
                args: vec![],
                dir,
                compile_log,
            })
        }
        "c" => {
            let args = vec![
                "-o".into(),
                binary.clone().into(),
                source.clone().into(),
                "-lm".into(),
            ];
            let compile_log = compile(&["cc", "gcc", "clang"], args, &dir, lang)?;
            Ok(Program {
                cmd: binary,
                args: vec![],
                dir,
                compile_log,
            })
        }
        "java" => {
            // javac writes Main.class for `class Main` into its own directory.
            let classes = build_dir.join(format!("{stem}-{hash:x}-classes"));
            let _ = std::fs::remove_dir_all(&classes);
            std::fs::create_dir_all(&classes).map_err(|e| e.to_string())?;
            let args = vec![
                "-encoding".into(),
                "UTF-8".into(),
                "-d".into(),
                classes.clone().into(),
                source.clone().into(),
            ];
            let compile_log = compile(&["javac"], args, &dir, lang)?;
            let java = find_tool(&["java"]).ok_or_else(|| not_found("java", lang))?;
            Ok(Program {
                cmd: java,
                args: vec![
                    "-Dstdout.encoding=UTF-8".into(),
                    "-cp".into(),
                    classes.into(),
                    stem.into(),
                ],
                dir,
                compile_log,
            })
        }
        "py" => interpreted(&["python3", "python"]),
        "js" => interpreted(&["node"]),
        _ => Err(tr!(
            lang,
            "don't know how to run '.{ext}' files (supported: {SUPPORTED})",
            "weiß nicht, wie man '.{ext}'-Dateien ausführt (unterstützt: {SUPPORTED})",
        )),
    }
}

impl Program {
    pub fn run(&self, input: &str, timeout: Duration, lang: Lang) -> Result<RunResult, String> {
        let start = Instant::now();
        let mut child = Command::new(&self.cmd)
            .args(&self.args)
            .current_dir(&self.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                tr!(
                    lang,
                    "could not start program: {e}",
                    "Programm konnte nicht gestartet werden: {e}"
                )
            })?;

        let mut stdin = child.stdin.take().expect("stdin is piped");
        let input = input.to_owned();
        let writer = thread::spawn(move || {
            // The program may exit without reading its input; that's fine.
            let _ = stdin.write_all(input.as_bytes());
        });
        let reader = |pipe: Option<Box<dyn Read + Send>>| {
            thread::spawn(move || {
                let mut buf = Vec::new();
                if let Some(pipe) = pipe {
                    let _ = pipe.take(MAX_OUTPUT).read_to_end(&mut buf);
                }
                String::from_utf8_lossy(&buf).into_owned()
            })
        };
        let out = reader(child.stdout.take().map(|p| Box::new(p) as _));
        let err = reader(child.stderr.take().map(|p| Box::new(p) as _));

        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break Some(status);
            }
            if start.elapsed() > timeout {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break None;
            }
            thread::sleep(Duration::from_millis(5));
        };
        let _ = writer.join();
        Ok(RunResult {
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
            code: status.and_then(|s| s.code()),
            timed_out,
        })
    }
}

/// Output comparison ignores trailing whitespace on lines and trailing empty lines.
pub fn normalize(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    let end = lines
        .iter()
        .rposition(|l| !l.is_empty())
        .map_or(0, |i| i + 1);
    lines[..end].join("\n")
}
