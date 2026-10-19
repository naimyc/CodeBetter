// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod i18n;
mod runner;
mod shell;
mod tasks;

use std::sync::{Arc, Mutex, MutexGuard};

use shell::{Action, Completion, EditorCheck, Response, Shell, TaskContext};
use tauri::{AppHandle, Manager, State};

type SharedShell = Arc<Mutex<Shell>>;

fn lock(shell: &SharedShell) -> MutexGuard<'_, Shell> {
    shell.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[tauri::command]
fn welcome(state: State<'_, SharedShell>) -> Response {
    lock(&state).welcome()
}

/// Commands may compile and run code, so they run off the main thread.
#[tauri::command]
async fn exec(app: AppHandle, state: State<'_, SharedShell>, line: String) -> Result<Response, String> {
    let shell = state.inner().clone();
    let response = tauri::async_runtime::spawn_blocking(move || lock(&shell).exec(&line))
        .await
        .map_err(|e| e.to_string())?;
    if matches!(response.action, Some(Action::Exit)) {
        app.exit(0);
    }
    Ok(response)
}

#[tauri::command]
fn complete(state: State<'_, SharedShell>, line: String) -> Completion {
    lock(&state).complete(&line)
}

#[tauri::command]
fn save(state: State<'_, SharedShell>, path: String, content: String) -> Result<(), String> {
    lock(&state).save(&path, &content)
}

#[tauri::command]
fn editor_hint(state: State<'_, SharedShell>, path: String) -> Option<TaskContext> {
    lock(&state).editor_hint(&path)
}

/// Compiles and tests, so like `exec` it runs off the main thread.
#[tauri::command]
async fn editor_check(state: State<'_, SharedShell>, path: String) -> Result<EditorCheck, String> {
    let shell = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || lock(&shell).editor_check(&path))
        .await
        .map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let user: String = std::env::var("USER")
                .unwrap_or_default()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            let user = if user.is_empty() { "dev".to_string() } else { user };
            let shell = Shell::new(data_dir.join("fs"), &data_dir, user, tasks::load_catalog())?;
            app.manage::<SharedShell>(Arc::new(Mutex::new(shell)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![welcome, exec, complete, save, editor_hint, editor_check])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
