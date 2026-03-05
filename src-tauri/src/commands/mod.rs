use std::sync::MutexGuard;

use rusqlite::Connection;
use tauri::State;

use crate::app;
use crate::domain::OverviewResponse;
use crate::infra::{AppError, AppResult, AppState};

fn lock_db<'a>(state: &'a State<'_, AppState>) -> AppResult<MutexGuard<'a, Connection>> {
    state
        .db
        .lock()
        .map_err(|_| AppError::internal("failed to lock database state", "poisoned mutex"))
}

#[tauri::command]
pub fn ping() -> String {
    "pong".to_string()
}

#[tauri::command]
pub fn get_overview(
    state: State<'_, AppState>,
    range: Option<String>,
) -> AppResult<OverviewResponse> {
    let conn = lock_db(&state)?;
    app::get_overview(&conn, range)
}

#[tauri::command]
pub fn create_task(
    state: State<'_, AppState>,
    title: String,
    parent_id: Option<String>,
) -> AppResult<String> {
    let mut conn = lock_db(&state)?;
    app::create_task(&mut conn, title, parent_id)
}

#[tauri::command]
pub fn rename_task(state: State<'_, AppState>, task_id: String, title: String) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::rename_task(&mut conn, task_id, title)
}

#[tauri::command]
pub fn archive_task(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::archive_task(&mut conn, task_id)
}

#[tauri::command]
pub fn delete_tasks(
    state: State<'_, AppState>,
    task_ids: Vec<String>,
    hard_delete: bool,
) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::delete_tasks(&mut conn, task_ids, hard_delete)
}

#[tauri::command]
pub fn reparent_task(
    state: State<'_, AppState>,
    task_id: String,
    new_parent_id: Option<String>,
) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::reparent_task(&mut conn, task_id, new_parent_id)
}

#[tauri::command]
pub fn start_task(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::start_task(&mut conn, task_id)
}

#[tauri::command]
pub fn pause_task(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::pause_task(&mut conn, task_id)
}

#[tauri::command]
pub fn resume_task(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::resume_task(&mut conn, task_id)
}

#[tauri::command]
pub fn stop_task(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::stop_task(&mut conn, task_id)
}

#[tauri::command]
pub fn insert_subtask_and_start(
    state: State<'_, AppState>,
    parent_task_id: String,
    title: String,
) -> AppResult<String> {
    let mut conn = lock_db(&state)?;
    app::insert_subtask_and_start(&mut conn, parent_task_id, title)
}

#[tauri::command]
pub fn add_tag_to_task(
    state: State<'_, AppState>,
    task_id: String,
    tag_name: String,
) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::add_tag_to_task(&mut conn, task_id, tag_name)
}

#[tauri::command]
pub fn remove_tag_from_task(
    state: State<'_, AppState>,
    task_id: String,
    tag_name: String,
) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::remove_tag_from_task(&mut conn, task_id, tag_name)
}

#[tauri::command]
pub fn respond_rest_suggestion(
    state: State<'_, AppState>,
    suggestion_id: i64,
    accept: bool,
) -> AppResult<()> {
    let mut conn = lock_db(&state)?;
    app::respond_rest_suggestion(&mut conn, suggestion_id, accept)
}
