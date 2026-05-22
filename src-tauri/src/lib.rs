//! Entry point shared between `main.rs` (release) and mobile targets.

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
