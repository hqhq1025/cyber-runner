//! fake-run Tauri host 入口。
//!
//! Setup 流程：
//! 1. 选引擎后端：
//!    - macOS + legacy RunControl helper 已安装并跑着 → `LegacyHelperEngine`
//!      （走 file-IPC 给 root LaunchDaemon，与 Swift Run Control 同款通道）
//!    - 否则 → `RealEngine`（在 Tauri 进程内直接 spawn python，
//!      mac 这种 case 上跑不通因为没 root；留给 Linux / 未来 Windows 用）
//! 2. 包成 `Arc<dyn Engine>` 给 `AppState`
//! 3. 在 setup 里 spawn 一个 tokio task：订阅 engine events，逐条 `apply_event`
//!    并 `app_handle.emit("engine_event", ...)`

mod commands;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use fake_run_engine::{legacy_helper_available, Engine, LegacyHelperEngine, RealEngine};
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

/// 选最合适的引擎后端。
/// 优先级：macOS legacy helper > RealEngine direct-spawn。
fn pick_engine() -> Arc<dyn Engine> {
    if cfg!(target_os = "macos") && legacy_helper_available() {
        tracing::info!("using LegacyHelperEngine (legacy Swift RunControl helper detected)");
        Arc::new(LegacyHelperEngine::new()) as Arc<dyn Engine>
    } else {
        let runtime = default_runtime_dir();
        tracing::info!(?runtime, "using RealEngine (direct python spawn)");
        Arc::new(RealEngine::new(runtime)) as Arc<dyn Engine>
    }
}

pub fn run() {
    let base_dir = default_base_dir();
    let engine = pick_engine();
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
