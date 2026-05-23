//! Cyber Runner Tauri host 入口。
//!
//! Setup 流程：
//! 1. 选引擎后端：
//!    - macOS + legacy RunControl helper 已安装并跑着 → `LegacyHelperEngine`
//!      （走 file-IPC 给 root LaunchDaemon —— **过渡期**寄生路径，
//!      `~/Library/Application Support/RunControl/commands/` 由 legacy helper
//!      监听；我们在这条通道上发请求。Cyber Runner 自己的 helper
//!      install flow 是下一阶段的事）
//!    - 否则 → `RealEngine`（在 Tauri 进程内直接 spawn python，
//!      mac 没 root 跑不通，留给 Linux / 未来 Windows 用）
//! 2. AppState 自己的 log/pid 路径走 **CyberRunner/**（与 Cyber Runner 品牌一致）
//! 3. setup 里 spawn 一个 tokio task：订阅 engine events，逐条 `apply_event`
//!    并 `app_handle.emit("engine_event", ...)`

mod commands;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use cyber_runner_engine::{legacy_helper_available, Engine, LegacyHelperEngine, RealEngine};
use state::AppState;
use tauri::{Emitter, Manager};

/// Cyber Runner 自己的 base dir —— log / pid / 未来 own helper 都在这里。
const APP_BASE_REL: &str = "Library/Application Support/CyberRunner";
const APP_RUNTIME_REL: &str = "Library/Application Support/CyberRunner/runtime";

fn default_base_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(APP_BASE_REL)
}

fn default_runtime_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(APP_RUNTIME_REL)
}

/// 选最合适的引擎后端。
/// 优先级：macOS legacy helper（兼容现有 Run Control 安装）> RealEngine direct-spawn。
fn pick_engine() -> Arc<dyn Engine> {
    if cfg!(target_os = "macos") && legacy_helper_available() {
        tracing::info!(
            "using LegacyHelperEngine — bridging via legacy RunControl LaunchDaemon \
             (Cyber Runner own helper install is pending)"
        );
        Arc::new(LegacyHelperEngine::new()) as Arc<dyn Engine>
    } else {
        let runtime = default_runtime_dir();
        tracing::info!(?runtime, "using RealEngine (direct python spawn)");
        Arc::new(RealEngine::new(runtime)) as Arc<dyn Engine>
    }
}

pub fn run() {
    let base_dir = default_base_dir();
    // 首次启动确保我们自己的 base dir 在
    if let Err(err) = std::fs::create_dir_all(&base_dir) {
        tracing::warn!(?err, ?base_dir, "could not create CyberRunner base dir");
    }
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
