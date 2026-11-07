mod commands;
pub mod db;
pub mod error;
pub mod ingest;
mod state;
pub mod usage;

use std::sync::{Mutex, mpsc};

use tauri::Manager;

use state::{AppState, IngestState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
                app.handle().plugin(tauri_plugin_process::init())?;
            }
            let data_dir = app.path().app_data_dir()?;
            // Debug builds keep their own archive so development can never
            // migrate the installed app's archive past what it understands.
            #[cfg(debug_assertions)]
            let data_dir = data_dir.join("dev");
            let db_path = data_dir.join(db::DB_FILE_NAME);
            // A failed open (for example an archive from a newer version) must
            // not stop the window appearing: the UI explains the problem.
            let (conn, db_error) = match db::open(&db_path) {
                Ok(c) => (Some(c), None),
                Err(e) => (None, Some(e.to_string())),
            };
            let can_ingest = conn.is_some();
            let (ingest_tx, ingest_rx) = mpsc::channel();
            let (usage_tx, usage_rx) = mpsc::channel();
            app.manage(AppState {
                db_path,
                db: Mutex::new(conn),
                db_error,
                ingest: Mutex::new(IngestState::Idle),
                ingest_tx: ingest_tx.clone(),
                plan_usage: Mutex::new(None),
                usage_tx,
            });
            if can_ingest {
                ingest::runner::spawn(app.handle().clone(), ingest_tx, ingest_rx);
                usage::spawn(app.handle().clone(), usage_rx);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::app_info,
            commands::settings::get_settings,
            commands::settings::set_projects_root,
            commands::settings::set_stale_minutes,
            commands::settings::set_update_check_on_startup,
            commands::settings::set_plan_usage,
            commands::usage::get_plan_usage,
            commands::usage::refresh_plan_usage,
            commands::usage::token_usage,
            commands::ingest::ingest_state,
            commands::sessions::list_projects,
            commands::sessions::archive_info,
            commands::tasks::list_tasks,
            commands::tasks::get_task,
            commands::tasks::get_entry_text,
            commands::tasks::search_entries,
        ])
        .run(tauri::generate_context!());

    // Nothing useful can run if the event loop fails to start, so report and exit.
    if let Err(err) = result {
        eprintln!("error while running application: {err}");
        std::process::exit(1);
    }
}
