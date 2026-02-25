use tauri::Manager;

mod app;
mod commands;
mod domain;
mod infra;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let state = infra::AppState::initialize(&app.handle()).map_err(|error| {
                std::io::Error::other(format!("failed to initialize app state: {error}"))
            })?;
            app.manage(state);
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_overview,
            commands::create_task,
            commands::rename_task,
            commands::archive_task,
            commands::delete_tasks,
            commands::reparent_task,
            commands::start_task,
            commands::pause_task,
            commands::resume_task,
            commands::stop_task,
            commands::insert_subtask_and_start,
            commands::add_tag_to_task,
            commands::remove_tag_from_task,
            commands::respond_rest_suggestion
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
