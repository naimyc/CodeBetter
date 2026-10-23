//! Creates new courses with an AI. It talks to any OpenAI-compatible chat
//! API; the default is KI:connect.nrw, the AI service of the universities
//! in North Rhine-Westphalia.
//!
//! Settings come from `<root>/.ki.toml`, and environment variables override
//! them:
//!
//! ```toml
//! provider = "kiconnect"     # or "openai", or a name for any other API
//! base_url = "https://chat.kiconnect.nrw/api/v1"   # needed for other APIs
//! model = "..."              # optional; see below
//! api_key = "..."            # or CB_AI_KEY / KICONNECT_API_KEY
//! ```
//!
//! For KI:connect.nrw the key alone is enough: a file with just the key.
//! Its model is then the first of `KICONNECT_MODELS` (the models for
//! students), and M on the start screen switches to the next one. Other
//! APIs without a model use the first one their `/models` lists.
//!
//! The AI writes the course with a reference solution per lesson. Every
//! solution must pass its lesson's tests before the course is kept, and
//! one failed check goes back to the AI to be fixed.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::catalog::{self, Task};
use crate::check;
use crate::i18n::Lang;
use crate::tr;

pub const CONFIG_FILE: &str = ".ki.toml";
const KICONNECT_URL: &str = "https://chat.kiconnect.nrw/api/v1";
const OPENAI_URL: &str = "https://api.openai.com/v1";
/// The chat models KI:connect.nrw offers students: id and name.
const KICONNECT_MODELS: &[(&str, &str)] = &[
    ("qwen3.5-397b-a17b_stud", "Qwen 3.5 397B"),
    ("glm-4.7_stud", "GLM 4.7"),
    ("qwen3.8_stud", "Qwen 3.8"),
    ("mistralai-mistral-small-4_STUD", "Mistral Small 4"),
    ("gemma-3-27b-it_stud", "Gemma 3 27B"),
];
/// Thinking models can be quiet for a while before the first token.
const READ_TIMEOUT: Duration = Duration::from_secs(180);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// How often the start screen hears about new tokens.
const UPDATE_EVERY: Duration = Duration::from_millis(80);
const ATTEMPTS: usize = 2;

#[derive(Serialize, Deserialize, Default)]
struct ConfigFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

/// Names a key may have on a line of its own: `KEY=...`, `export
/// KICONNECT_API_KEY=...` (as in a `.env` file) and the like.
const KEY_NAMES: [&str; 4] = ["api_key", "key", "kiconnect_api_key", "cb_ai_key"];

impl ConfigFile {
    /// The settings, or just the key. Keys can contain `=`, `/` and `:`,
    /// so a single line is the key itself unless it is a TOML setting:
    ///
    /// ```text
    /// UKG...:ab/cd==                 the key alone
    /// KEY=UKG...:ab/cd==             the key with a name, as in a .env file
    /// api_key = "UKG...:ab/cd=="     TOML, with more settings on other lines
    /// ```
    fn parse(text: &str) -> Self {
        let text = text.trim_start_matches('\u{feff}').trim();
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        let only_key = |key: String| Self {
            api_key: Some(key),
            ..Self::default()
        };
        if let Ok(cfg) = toml::from_str::<Self>(text)
            && (cfg.api_key.is_some()
                || cfg.model.is_some()
                || cfg.provider.is_some()
                || cfg.base_url.is_some())
        {
            return cfg;
        }
        if let Some(key) = lines.iter().find_map(|l| named_key(l)) {
            return only_key(key);
        }
        match lines[..] {
            [line] if !line.contains(char::is_whitespace) => only_key(unquote(line).to_owned()),
            _ => Self::default(),
        }
    }
}

/// The value of a line like `KEY=...`, if it names the key.
fn named_key(line: &str) -> Option<String> {
    let line = line.strip_prefix("export ").unwrap_or(line);
    let (name, value) = line.split_once('=')?;
    let value = unquote(value.trim());
    (KEY_NAMES.contains(&name.trim().to_lowercase().as_str()) && !value.is_empty())
        .then(|| value.to_owned())
}

fn unquote(s: &str) -> &str {
    s.trim_matches(['"', '\''])
}

pub struct Config {
    /// Shown on the start screen, e.g. `KI:connect.nrw`.
    pub name: String,
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    /// Known chat models of the provider, to switch between.
    models: &'static [(&'static str, &'static str)],
    pub file: PathBuf,
}

impl Config {
    pub fn load(root: &Path) -> Self {
        let file = root.join(CONFIG_FILE);
        let cfg = std::fs::read_to_string(&file)
            .map(|t| ConfigFile::parse(&t))
            .unwrap_or_default();
        let env = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let provider = env("CB_AI_PROVIDER")
            .or(cfg.provider)
            .unwrap_or_else(|| "kiconnect".into());
        let (name, preset, models) = match provider.to_lowercase().as_str() {
            "kiconnect" | "kiconnect.nrw" | "ki:connect.nrw" => (
                "KI:connect.nrw".into(),
                Some(KICONNECT_URL),
                KICONNECT_MODELS,
            ),
            "openai" => ("OpenAI".into(), Some(OPENAI_URL), &[][..]),
            _ => (provider, None, &[][..]),
        };
        Self {
            name,
            base_url: env("CB_AI_BASE_URL")
                .or(cfg.base_url)
                .or(preset.map(Into::into)),
            model: env("CB_AI_MODEL")
                .or(cfg.model)
                .or(models.first().map(|(id, _)| (*id).to_owned())),
            api_key: env("CB_AI_KEY")
                .or_else(|| env("KICONNECT_API_KEY"))
                .or(cfg.api_key),
            models,
            file,
        }
    }

    /// What is still missing before the AI can be used.
    pub fn missing(&self, lang: Lang) -> Option<String> {
        let file = crate::workspace::pretty(&self.file);
        if self.base_url.is_none() {
            Some(tr!(
                lang,
                "No API address: set base_url in {file} or CB_AI_BASE_URL.",
                "Keine API-Adresse: trag base_url in {file} ein oder setze CB_AI_BASE_URL."
            ))
        } else if self.api_key.is_none() {
            Some(tr!(
                lang,
                "No API key: S stores it (in {file}), or set CB_AI_KEY.",
                "Kein API-Schlüssel: S legt ihn ab (in {file}), oder setze CB_AI_KEY."
            ))
        } else {
            None
        }
    }

    /// The model as the start screen shows it: `Qwen 3.5 397B (qwen3.5-...)`.
    pub fn model_name(&self) -> Option<String> {
        let id = self.model.as_deref()?;
        Some(match self.models.iter().find(|(m, _)| *m == id) {
            Some((_, name)) => format!("{name} ({id})"),
            None => id.to_owned(),
        })
    }

    /// Whether there are models to switch between.
    pub fn can_switch_model(&self) -> bool {
        self.models.len() > 1
    }

    /// Switches to the next known model and saves it in the settings file,
    /// keeping what else is in there.
    pub fn next_model(&mut self) -> Result<(), String> {
        let at = self
            .models
            .iter()
            .position(|(id, _)| Some(*id) == self.model.as_deref());
        let next = at.map_or(0, |i| (i + 1) % self.models.len());
        let Some((id, _)) = self.models.get(next) else {
            return Ok(());
        };
        self.update_file(|cfg| cfg.model = Some((*id).to_owned()))?;
        self.model = Some((*id).to_owned());
        Ok(())
    }

    /// Saves `key` as the API key in the settings file, keeping what else
    /// is in there.
    pub fn save_key(&mut self, key: &str) -> Result<(), String> {
        let key = unquote(key.trim()).to_owned();
        self.update_file(|cfg| cfg.api_key = Some(key.clone()))?;
        self.api_key = Some(key);
        Ok(())
    }

    /// Rewrites the settings file as TOML with `change` applied. Only the
    /// owner can read it: it holds the key.
    fn update_file(&self, change: impl FnOnce(&mut ConfigFile)) -> Result<(), String> {
        let shown = crate::workspace::pretty(&self.file);
        let mut cfg = std::fs::read_to_string(&self.file)
            .map(|t| ConfigFile::parse(&t))
            .unwrap_or_default();
        change(&mut cfg);
        let text = toml::to_string(&cfg).map_err(|e| e.to_string())?;
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{shown}: {e}"))?;
        }
        std::fs::write(&self.file, format!("# CBEditor: AI settings\n{text}"))
            .map_err(|e| format!("{shown}: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.file, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// Whether a key comes from the environment, which beats the file.
    pub fn key_from_env() -> bool {
        ["CB_AI_KEY", "KICONNECT_API_KEY"]
            .iter()
            .any(|v| std::env::var(v).is_ok_and(|k| !k.trim().is_empty()))
    }
}

/// What the AI is doing right now, for the start screen. It shows small
/// pieces of the answer while it streams in, like a chat UI.
#[derive(Clone, Default)]
pub struct Activity {
    /// Like "KI:connect.nrw (glm-4.7_stud) schreibt den Kurs".
    pub step: String,
    /// The model is still reasoning and hasn't started the answer.
    pub thinking: bool,
    /// Characters received in this step, reasoning included.
    pub chars: usize,
    /// The lesson being written, like "Lektion 2: Schleifen".
    pub lesson: Option<String>,
    /// The last line or two that came in.
    pub preview: Vec<String>,
}

/// A piece of a streamed answer.
enum Delta<'a> {
    Thinking(&'a str),
    Text(&'a str),
}

struct Client<'a> {
    config: &'a Config,
    agent: ureq::Agent,
    lang: Lang,
}

impl Client<'_> {
    fn url(&self, path: &str) -> String {
        let base = self.config.base_url.as_deref().unwrap_or_default();
        format!("{}/{path}", base.trim_end_matches('/'))
    }

    fn auth(&self) -> String {
        format!(
            "Bearer {}",
            self.config.api_key.as_deref().unwrap_or_default()
        )
    }

    /// What went wrong with request `what`, with the server's own reason.
    fn error(&self, what: &str, e: ureq::Error) -> String {
        let lang = self.lang;
        match e {
            ureq::Error::Status(code, response) => {
                let body = response.into_string().unwrap_or_default();
                let reason = server_reason(&body);
                if matches!(code, 401 | 403) {
                    let file = crate::workspace::pretty(&self.config.file);
                    tr!(
                        lang,
                        "{what}: API key refused ({code}: {reason}). Check the key in {file}.",
                        "{what}: API-Schlüssel abgelehnt ({code}: {reason}). Prüf den Schlüssel in {file}."
                    )
                } else {
                    tr!(
                        lang,
                        "{what}: API error {code}: {reason}",
                        "{what}: API-Fehler {code}: {reason}"
                    )
                }
            }
            ureq::Error::Transport(t) => tr!(
                lang,
                "{what}: no connection to the API: {t}",
                "{what}: keine Verbindung zur API: {t}"
            ),
        }
    }

    /// The configured model, or the first one the API offers.
    fn model(&self) -> Result<String, String> {
        if let Some(model) = &self.config.model {
            return Ok(model.clone());
        }
        let list: Value = self
            .agent
            .get(&self.url("models"))
            .set("Authorization", &self.auth())
            .call()
            .map_err(|e| self.error("GET /models", e))?
            .into_json()
            .map_err(|e| e.to_string())?;
        list["data"][0]["id"]
            .as_str()
            .map(Into::into)
            .ok_or_else(|| {
                self.lang
                    .pick(
                        "the API lists no models; set model in the settings",
                        "die API nennt keine Modelle; trag model in den Einstellungen ein",
                    )
                    .into()
            })
    }

    /// Sends the chat and streams the answer: `on_delta` gets every piece
    /// as it arrives. Returns the whole answer. APIs that don't stream send
    /// it in one go, which works too.
    fn chat(
        &self,
        model: &str,
        messages: &[Value],
        cancel: &AtomicBool,
        on_delta: &mut dyn FnMut(Delta),
    ) -> Result<String, String> {
        let lang = self.lang;
        let body = json!({
            "model": model, "messages": messages, "temperature": 0.3, "stream": true,
        });
        let response = self
            .agent
            .post(&self.url("chat/completions"))
            .set("Authorization", &self.auth())
            .set("Accept", "text/event-stream")
            .send_json(body)
            .map_err(|e| self.error("POST /chat/completions", e))?;
        let mut text = String::new();
        let mut finish = Value::Null;
        let mut plain = String::new();
        for line in BufReader::new(response.into_reader()).lines() {
            if cancel.load(Ordering::Relaxed) {
                return Err(lang.pick("cancelled", "abgebrochen").into());
            }
            let line = line.map_err(|e| {
                tr!(
                    lang,
                    "the answer broke off: {e}",
                    "die Antwort brach ab: {e}"
                )
            })?;
            let Some(data) = line.strip_prefix("data:").map(str::trim) else {
                // Not a stream: the whole answer as one JSON body.
                plain.push_str(&line);
                plain.push('\n');
                continue;
            };
            if data == "[DONE]" {
                break;
            }
            let Ok(chunk) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            let choice = &chunk["choices"][0];
            let delta = &choice["delta"];
            for key in ["reasoning", "reasoning_content"] {
                if let Some(t) = delta[key].as_str().filter(|t| !t.is_empty()) {
                    on_delta(Delta::Thinking(t));
                }
            }
            if let Some(t) = delta["content"].as_str().filter(|t| !t.is_empty()) {
                text.push_str(t);
                on_delta(Delta::Text(t));
            }
            if !choice["finish_reason"].is_null() {
                finish = choice["finish_reason"].clone();
            }
        }
        if text.is_empty() && !plain.trim().is_empty() {
            let reply: Value = serde_json::from_str(&plain).map_err(|e| e.to_string())?;
            return answer(&reply, lang);
        }
        let reply =
            json!({ "choices": [{ "finish_reason": finish, "message": { "content": text } }] });
        answer(&reply, lang)
    }
}

/// Asks the AI for a course about `topic`, written in `lang`, and checks
/// it. `progress` hears what is going on, many times a second while the
/// answer streams in. Setting `cancel` stops at the next piece.
pub fn generate(
    config: &Config,
    topic: &str,
    lang: Lang,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&Activity),
) -> Result<Task, String> {
    if let Some(missing) = config.missing(lang) {
        return Err(missing);
    }
    let client = Client {
        config,
        agent: ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout_read(READ_TIMEOUT)
            .user_agent(concat!("cbeditor/", env!("CARGO_PKG_VERSION")))
            .build(),
        lang,
    };
    let name = &config.name;
    let step = |step: String| Activity {
        step,
        ..Activity::default()
    };
    progress(&step(tr!(lang, "asking {name} ...", "frage {name} ...")));
    let model = client.model()?;
    let mut messages = vec![
        json!({ "role": "system", "content": system_prompt(lang) }),
        json!({ "role": "user", "content": topic }),
    ];
    let mut problem = String::new();
    for attempt in 1..=ATTEMPTS {
        let mut activity = step(if attempt == 1 {
            tr!(
                lang,
                "{name} ({model}) is writing the course",
                "{name} ({model}) schreibt den Kurs"
            )
        } else {
            tr!(
                lang,
                "{name} ({model}) is fixing the course, attempt {attempt}/{ATTEMPTS}",
                "{name} ({model}) bessert den Kurs nach, Versuch {attempt}/{ATTEMPTS}"
            )
        });
        progress(&activity);
        let mut thoughts = String::new();
        let mut text = String::new();
        let mut last = Instant::now();
        let reply = client.chat(&model, &messages, cancel, &mut |delta| {
            match delta {
                Delta::Thinking(t) => {
                    thoughts.push_str(t);
                    activity.thinking = text.is_empty();
                    activity.chars += t.chars().count();
                    if activity.thinking {
                        activity.preview = tail_lines(&thoughts, 1);
                    }
                }
                Delta::Text(t) => {
                    text.push_str(t);
                    activity.thinking = false;
                    activity.chars += t.chars().count();
                    activity.lesson = current_lesson(&text, lang);
                    activity.preview = tail_lines(&text, 2);
                }
            }
            if last.elapsed() >= UPDATE_EVERY {
                progress(&activity);
                last = Instant::now();
            }
        })?;
        match accept(&reply, lang, &mut |s| progress(&step(s.to_owned()))) {
            Ok(task) => return Ok(task),
            Err(why) => problem = why,
        }
        messages.push(json!({ "role": "assistant", "content": reply }));
        messages.push(json!({
            "role": "user",
            "content": format!(
                "The course is not usable yet:\n{problem}\n\n\
                 Fix it and send the whole corrected file again, TOML only."
            ),
        }));
    }
    Err(tr!(
        lang,
        "the AI's course didn't pass its own check: {problem}",
        "der Kurs der KI hat die eigene Prüfung nicht bestanden: {problem}"
    ))
}

/// The last `n` lines of `text` that have something in them.
fn tail_lines(text: &str, n: usize) -> Vec<String> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| l.replace('\t', "    ").trim_end().to_owned())
        .collect()
}

/// Which lesson of the course file is being written: the number of
/// `[[task.part]]` so far and the last one's title, once it's there.
fn current_lesson(text: &str, lang: Lang) -> Option<String> {
    let n = text.matches("[[task.part]]").count();
    if n == 0 {
        return Some(lang.pick("course outline", "Kursrahmen").into());
    }
    let part = &text[text.rfind("[[task.part]]")?..];
    let title = part.lines().find_map(|l| {
        let value = l
            .trim()
            .strip_prefix("title")?
            .trim_start()
            .strip_prefix('=')?;
        let value = value.trim().trim_matches('"');
        (!value.is_empty()).then(|| value.to_owned())
    });
    let lesson = tr!(lang, "Lesson {n}", "Lektion {n}");
    Some(match title {
        Some(title) => format!("{lesson}: {title}"),
        None => lesson,
    })
}

/// Reads the AI's reply as a course and runs every lesson's reference
/// solution against its tests.
fn accept(reply: &str, lang: Lang, progress: &mut dyn FnMut(&str)) -> Result<Task, String> {
    let mut task = catalog::parse(strip_fences(reply), lang)?;
    task.id = catalog::new_id("ki", task.title.get(lang));
    let dir = std::env::temp_dir().join(format!("cbeditor-ki-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(&task.starter_file);
    let total = task.parts.len();
    for (i, part) in task.parts.iter().enumerate() {
        let n = i + 1;
        progress(&tr!(
            lang,
            "checking lesson {n}/{total} with the reference solution ...",
            "prüfe Lektion {n}/{total} mit der Musterlösung ..."
        ));
        let Some(solution) = &part.solution else {
            return Err(format!("lesson {n} has no `solution`"));
        };
        std::fs::write(&file, solution).map_err(|e| e.to_string())?;
        let (report, passed) = check::check(&file, &part.tests, &part.requires, lang);
        if !passed {
            let report: Vec<String> = report.into_iter().map(|(line, _)| line).collect();
            return Err(format!(
                "lesson {n}: the solution fails its own lesson:\n{}",
                report.join("\n")
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(task)
}

/// The text of a chat reply. Thinking models (Qwen on KI:connect) put
/// their reasoning in `message.reasoning` and the answer in `content`;
/// a reply cut off at the token limit has no usable answer.
fn answer(reply: &Value, lang: Lang) -> Result<String, String> {
    let choice = &reply["choices"][0];
    let content = choice["message"]["content"].as_str().unwrap_or_default();
    if choice["finish_reason"] == "length" {
        return Err(lang
            .pick(
                "the model's answer was cut off at its token limit; try another model (M)",
                "die Antwort des Modells wurde am Token-Limit abgeschnitten; versuch ein anderes Modell (M)",
            )
            .into());
    }
    if content.trim().is_empty() {
        return Err(lang
            .pick(
                "the API sent no answer",
                "die API hat keine Antwort geschickt",
            )
            .into());
    }
    Ok(content.to_owned())
}

/// The message of an OpenAI-style error (`{"error": {"message": ..}}`),
/// or the start of the body.
fn server_reason(body: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let message = parsed.as_ref().and_then(|v| {
        v["error"]["message"]
            .as_str()
            .or_else(|| v["error"].as_str())
            .or_else(|| v["message"].as_str())
            .or_else(|| v["detail"].as_str())
            .map(str::to_owned)
    });
    message
        .unwrap_or_else(|| body.trim().to_owned())
        .chars()
        .take(160)
        .collect()
}

/// The TOML inside a reply, without a ```toml fence around it.
fn strip_fences(reply: &str) -> &str {
    let text = reply.trim();
    let Some(start) = text.find("```") else {
        return text;
    };
    let body = &text[start + 3..];
    let body = body.find('\n').map_or(body, |i| &body[i + 1..]);
    body.rfind("```").map_or(body, |end| &body[..end]).trim()
}

fn system_prompt(lang: Lang) -> String {
    let language = lang.pick("English", "German");
    format!(
        r#"You write programming courses for CodeBetter, a platform where beginners
write every line themselves. The user names a topic and usually a
programming language. Reply with ONE course as a TOML file and nothing else:
no explanations, no Markdown fences.

Write all texts in {language}. Each course is one small program that grows
lesson by lesson: lesson N+1 extends the program of lesson N.

Format (follow it exactly):

format = 1

[task]
id = "course"
title = "Python: Lists"
summary = "One sentence for the course list."
starter_file = "main.py"
starter_code = '''
# A comment saying what the program will be and that lesson 1 starts here.
'''

[[task.part]]
title = "Lesson title"
lesson = '''
The concept, explained for beginners, with a small example that is NOT the
solution. Indent code examples by 4 spaces. Lines at most 70 characters.
'''
description = '''
What to add to the program, with the exact expected output for one input.
'''
hints = ["a nudge, never the solution", "a second, more concrete nudge"]
solution = '''
the complete program after this lesson; it must pass every test below
'''

[[task.part.require]]
code = "for "
message = "Use a for loop."

[[task.part.test]]
input = "3\n"
expected = '''
exact stdout of the program for this input
'''

[[task.part.test]]
input = "0\n"
hidden = true
expected = '''
...
'''

Rules:
- 3 to 5 lessons, each with 2 to 4 tests; at least one hidden edge case.
- starter_file: main.py (Python), Main.java (Java, `public class Main`),
  main.c (C), main.rs (Rust) or main.js (JavaScript). Python if none given.
- starter_code must run and print nothing.
- Programs read only stdin and write only stdout. No arguments, files,
  network, randomness, time or libraries beyond the standard library.
- `expected` is compared exactly (trailing spaces ignored), so make every
  output fully determined by the input.
- `require.code` checks that the learner's code uses the lesson's concept.
  It is matched as a sequence of tokens: words (names, keywords, numbers)
  and single symbols. Spacing never matters, so "int *" also matches
  `int*p` and `int * p`; words match only whole, so "for" doesn't match
  `format`; comments don't count. Use 1 or 2 per lesson and require only
  what every correct solution must contain: a keyword or construct like
  "while", "def", "int *", "malloc (", "->" or "f\"". Never require
  variable or function names, values or a particular style, unless the
  description tells the learner to use exactly that name.
- Use '''literal strings''' for text with several lines; they must not
  contain three single quotes in a row. In "basic strings", write \n for a
  line break inside `input`.
- The solution of every lesson must pass that lesson's tests and requires."#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fences_are_taken_off() {
        assert_eq!(strip_fences("```toml\nformat = 1\n```"), "format = 1");
        assert_eq!(strip_fences("Here:\n```\na = 1\n```\nDone."), "a = 1");
        assert_eq!(strip_fences("format = 1"), "format = 1");
    }

    #[test]
    fn kiconnect_is_the_default() {
        let config = Config::load(Path::new("/nonexistent"));
        if std::env::var_os("CB_AI_PROVIDER").is_none()
            && std::env::var_os("CB_AI_BASE_URL").is_none()
        {
            assert_eq!(config.name, "KI:connect.nrw");
            assert_eq!(config.base_url.as_deref(), Some(KICONNECT_URL));
        }
    }

    #[test]
    fn kiconnect_models_switch_and_the_key_stays() {
        let root = std::env::temp_dir().join(format!("cbeditor-ai-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(CONFIG_FILE), "\"geheim\"\n").unwrap();
        let env = [
            "CB_AI_MODEL",
            "CB_AI_PROVIDER",
            "CB_AI_KEY",
            "KICONNECT_API_KEY",
        ];
        if env.iter().any(|v| std::env::var_os(v).is_some()) {
            return;
        }
        let mut config = Config::load(&root);
        assert_eq!(config.model.as_deref(), Some("qwen3.5-397b-a17b_stud"));
        assert_eq!(
            config.model_name().as_deref(),
            Some("Qwen 3.5 397B (qwen3.5-397b-a17b_stud)")
        );
        config.next_model().unwrap();
        let mut again = Config::load(&root);
        assert_eq!(again.model.as_deref(), Some("glm-4.7_stud"));
        assert_eq!(again.api_key.as_deref(), Some("geheim"));
        again.save_key(" UKG1:a/b== ").unwrap();
        let last = Config::load(&root);
        assert_eq!(last.api_key.as_deref(), Some("UKG1:a/b=="));
        assert_eq!(
            last.model.as_deref(),
            Some("glm-4.7_stud"),
            "the model stays"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn shows_which_lesson_is_being_written() {
        assert_eq!(
            current_lesson("format = 1\n[task]\n", Lang::De).unwrap(),
            "Kursrahmen"
        );
        let text = "[[task.part]]\ntitle = \"A\"\n[[task.part]]\nlesson = x";
        assert_eq!(current_lesson(text, Lang::De).unwrap(), "Lektion 2");
        let text = "[[task.part]]\ntitle = \"A\"\n[[task.part]]\ntitle = \"Zeiger\"\n";
        assert_eq!(current_lesson(text, Lang::En).unwrap(), "Lesson 2: Zeiger");
        assert_eq!(tail_lines("a\n\n  b\nc  \n\n", 2), ["  b", "c"]);
    }

    #[test]
    fn thinking_and_cut_off_replies() {
        let reply = |content: Value, finish: &str| {
            json!({ "choices": [{ "finish_reason": finish,
                "message": { "content": content, "reasoning": "Thinking Process: ..." } }] })
        };
        assert_eq!(
            answer(&reply(json!("\n\nOK"), "stop"), Lang::De).unwrap(),
            "\n\nOK"
        );
        let cut = answer(&reply(Value::Null, "length"), Lang::De)
            .err()
            .unwrap();
        assert!(cut.contains("Token-Limit"), "{cut}");
        let half = answer(&reply(json!("format = 1\n[task"), "length"), Lang::De)
            .err()
            .unwrap();
        assert!(half.contains("abgeschnitten"), "{half}");
        let none = answer(&reply(Value::Null, "stop"), Lang::De).err().unwrap();
        assert!(none.contains("keine Antwort"), "{none}");
    }

    #[test]
    fn the_server_says_why() {
        let body = r#"{"error":{"message":"Invalid API key format format.","type":"authentication_error","code":"invalid_auth_header"}}"#;
        assert_eq!(server_reason(body), "Invalid API key format format.");
        assert_eq!(
            server_reason(r#"{"detail":"Not authenticated"}"#),
            "Not authenticated"
        );
        assert_eq!(server_reason("Forbidden"), "Forbidden");
    }

    #[test]
    fn the_file_can_hold_just_the_key() {
        let key = |text: &str| ConfigFile::parse(text).api_key;
        assert_eq!(key("sk-abc123\n").as_deref(), Some("sk-abc123"));
        // Longer keys with = / : in them, alone or named like in a .env file.
        let long = "UKGabc123:Zx/9+Qw==";
        assert_eq!(key(long).as_deref(), Some(long));
        assert_eq!(key(&format!("\u{feff}{long}\r\n")).as_deref(), Some(long));
        assert_eq!(key(&format!("KEY={long}")).as_deref(), Some(long));
        assert_eq!(
            key(&format!("export KICONNECT_API_KEY='{long}'")).as_deref(),
            Some(long)
        );
        assert_eq!(
            key(&format!("api_key = \"{long}\"\nmodel = \"m\"")).as_deref(),
            Some(long)
        );
        assert_eq!(key("model = \"glm-4.7_stud\""), None);
        assert_eq!(key("  \"sk-abc123\"  ").as_deref(), Some("sk-abc123"));
        assert_eq!(
            key("api_key = \"sk-1\"\nmodel = \"m\"").as_deref(),
            Some("sk-1")
        );
        assert_eq!(key(""), None);
        let full = ConfigFile::parse("provider = \"openai\"\napi_key = \"k\"");
        assert_eq!(full.provider.as_deref(), Some("openai"));
    }

    /// A course like the AI should send: one checked lesson.
    #[test]
    fn a_good_reply_is_accepted_and_a_wrong_solution_is_not() {
        let reply = r#"```toml
format = 1

[task]
id = "x"
title = "Python: Verdoppeln"
starter_file = "main.py"

[[task.part]]
title = "Doppelt"
description = "Gib die doppelte Zahl aus."
solution = '''
n = int(input())
print(n * 2)
'''

[[task.part.require]]
code = "input("
message = "Lies die Zahl."

[[task.part.test]]
input = "3\n"
expected = "6"
```"#;
        let task = accept(reply, Lang::De, &mut |_| {}).unwrap();
        assert!(task.id.starts_with("ki_python_verdoppeln"), "{}", task.id);
        let wrong = reply.replace("n * 2", "n * 3");
        let err = accept(&wrong, Lang::De, &mut |_| {}).err().unwrap();
        assert!(err.contains("lesson 1"), "{err}");
    }
}
