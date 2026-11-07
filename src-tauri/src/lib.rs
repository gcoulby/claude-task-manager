mod commands;
pub mod db;
pub mod error;
pub mod ingest;
mod state;

use std::sync::{Mutex, mpsc};

use tauri::Manager;

use state::{AppState, IngestState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let db_path = data_dir.join(db::DB_FILE_NAME);
            // A failed open (for example an archive from a newer version) must
            // not stop the window appearing: the UI explains the problem.
            let (conn, db_error) = match db::open(&db_path) {
                Ok(c) => (Some(c), None),
                Err(e) => (None, Some(e.to_string())),
            };
            let can_ingest = conn.is_some();
            let (ingest_tx, ingest_rx) = mpsc::channel();
            app.manage(AppState {
                db_path,
                db: Mutex::new(conn),
                db_error,
                ingest: Mutex::new(IngestState::Idle),
                ingest_tx: ingest_tx.clone(),
            });
            if can_ingest {
                ingest::runner::spawn(app.handle().clone(), ingest_tx, ingest_rx);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::app_info,
            commands::ingest::ingest_state,
        ])
        .run(tauri::generate_context!());

    // Nothing useful can run if the event loop fails to start, so report and exit.
    if let Err(err) = result {
        eprintln!("error while running application: {err}");
        std::process::exit(1);
    }
}
