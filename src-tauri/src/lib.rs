//! Entry point shared between `main.rs` (release) and mobile targets.

mod commands;
mod state;

use state::AppState;

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::start,
            commands::stop,
            commands::cleanup,
            commands::status,
        ])
        .setup(|_app| {
            tracing::info!("fake-run app booted (mock backend)");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
