use tauri::Manager;

mod app;
mod command_api;
mod command_catalog;
mod domain;
mod infra;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            command_catalog::load_builtin_command_catalog().map_err(|error| {
                std::io::Error::other(format!("failed to load command catalog: {error}"))
            })?;
            let state = infra::AppState::initialize(&app.handle()).map_err(|error| {
                std::io::Error::other(format!("failed to initialize app state: {error}"))
            })?;
            app.manage(state);
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            command_api::ping,
            command_api::get_command_catalog,
            command_api::get_overview,
            command_api::get_focus_summary,
            command_api::create_task,
            command_api::rename_task,
            command_api::archive_task,
            command_api::delete_tasks,
            command_api::reparent_task,
            command_api::start_task,
            command_api::pause_task,
            command_api::resume_task,
            command_api::stop_task,
            command_api::adjust_task_focus,
            command_api::insert_subtask_and_start,
            command_api::add_tag_to_task,
            command_api::remove_tag_from_task,
            command_api::respond_rest_suggestion
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            let pause_result = {
                let state = app_handle.state::<infra::AppState>();
                let mut conn = match state.db.lock() {
                    Ok(conn) => conn,
                    Err(_) => {
                        eprintln!("failed to pause running task on exit: poisoned db mutex");
                        return;
                    }
                };
                app::pause_running_task(&mut conn)
            };

            if let Err(error) = pause_result {
                eprintln!("failed to pause running task on exit: {error}");
            }
        }
    });
}
