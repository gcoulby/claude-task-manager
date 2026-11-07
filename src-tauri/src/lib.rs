mod commands;
mod error;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::settings::app_info])
        .run(tauri::generate_context!());

    // Nothing useful can run if the event loop fails to start, so report and exit.
    if let Err(err) = result {
        eprintln!("error while running application: {err}");
        std::process::exit(1);
    }
}
