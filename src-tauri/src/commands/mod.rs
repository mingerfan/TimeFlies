use tauri::State;

use crate::app;
use crate::domain::OverviewResponse;
use crate::infra::{AppResult, AppState};

#[tauri::command]
pub fn ping() -> String {
    "pong".to_string()
}

#[tauri::command]
pub fn get_overview(state: State<'_, AppState>, range: Option<String>) -> AppResult<OverviewResponse> {
    let conn = state
        .db
        .lock()
        .map_err(|_| "failed to lock database state".to_string())?;
    app::get_overview(&conn, range)
}

#[tauri::command]
pub fn create_task(
    state: State<'_, AppState>,
    title: String,
    parent_id: Option<String>,
) -> AppResult<String> {
    let mut conn = state
        .db
        .lock()
        .map_err(|_| "failed to lock database state".to_string())?;
    app::create_task(&mut conn, title, parent_id)
}

#[tauri::command]
pub fn start_task(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let mut conn = state
        .db
        .lock()
        .map_err(|_| "failed to lock database state".to_string())?;
    app::start_task(&mut conn, task_id)
}

#[tauri::command]
pub fn pause_task(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let mut conn = state
        .db
        .lock()
        .map_err(|_| "failed to lock database state".to_string())?;
    app::pause_task(&mut conn, task_id)
}

#[tauri::command]
pub fn resume_task(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let mut conn = state
        .db
        .lock()
        .map_err(|_| "failed to lock database state".to_string())?;
    app::resume_task(&mut conn, task_id)
}

#[tauri::command]
pub fn stop_task(state: State<'_, AppState>, task_id: String) -> AppResult<()> {
    let mut conn = state
        .db
        .lock()
        .map_err(|_| "failed to lock database state".to_string())?;
    app::stop_task(&mut conn, task_id)
}

#[tauri::command]
pub fn insert_subtask_and_start(
    state: State<'_, AppState>,
    parent_task_id: String,
    title: String,
) -> AppResult<String> {
    let mut conn = state
        .db
        .lock()
        .map_err(|_| "failed to lock database state".to_string())?;
    app::insert_subtask_and_start(&mut conn, parent_task_id, title)
}

#[tauri::command]
pub fn add_tag_to_task(state: State<'_, AppState>, task_id: String, tag_name: String) -> AppResult<()> {
    let mut conn = state
        .db
        .lock()
        .map_err(|_| "failed to lock database state".to_string())?;
    app::add_tag_to_task(&mut conn, task_id, tag_name)
}

#[tauri::command]
pub fn remove_tag_from_task(
    state: State<'_, AppState>,
    task_id: String,
    tag_name: String,
) -> AppResult<()> {
    let mut conn = state
        .db
        .lock()
        .map_err(|_| "failed to lock database state".to_string())?;
    app::remove_tag_from_task(&mut conn, task_id, tag_name)
}
