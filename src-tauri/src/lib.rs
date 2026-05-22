//! fake-run Tauri host 入口。
//!
//! Setup 流程：
//! 1. 解析 runtime_dir（沿用 legacy Run Control 在 `~/Library/Application Support/RunControl/runtime`
//!    的 venv 和 `main.py`）；如果用户希望换路径，未来从配置文件读
//! 2. 构造 `RealEngine` 包成 `Arc`，给 `AppState` 一份
//! 3. 在 setup 里 spawn 一个 tokio task：订阅 engine events，逐条 `apply_event`

mod commands;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use fake_run_engine::{Engine, RealEngine};
use state::AppState;
use tauri::{Emitter, Manager};

const DEFAULT_BASE_REL: &str = "Library/Application Support/RunControl";
const DEFAULT_RUNTIME_REL: &str = "Library/Application Support/RunControl/runtime";

fn default_base_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(DEFAULT_BASE_REL)
}

fn default_runtime_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(DEFAULT_RUNTIME_REL)
}

pub fn run() {
    let runtime_dir = default_runtime_dir();
    let base_dir = default_base_dir();
    tracing::info!(?runtime_dir, ?base_dir, "starting fake-run");

    let engine = Arc::new(RealEngine::new(runtime_dir));
    let app_state = AppState::new_real(engine.clone(), base_dir);

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::start,
            commands::stop,
            commands::cleanup,
            commands::status,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let mut rx = engine.subscribe();
            tauri::async_runtime::spawn(async move {
                tracing::info!("engine event pump started");
                while let Ok(event) = rx.recv().await {
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        state.apply_event(&event);
                    }
                    let _ = app_handle.emit("engine_event", &event);
                }
                tracing::warn!("engine event pump ended (sender dropped)");
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
