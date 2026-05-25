//! Cyber Runner Tauri host 入口。
//!
//! Setup 流程：
//! 1. 找 helper 二进制：
//!    - dev: `target/debug/cyber-runner-helper`（与 app 同目录）
//!    - release: `Cyber Runner.app/Contents/Resources/cyber-runner-helper`
//! 2. 选引擎后端：
//!    - 找到 helper → [`BundledHelperEngine`]（推荐路径，自家 root helper）
//!    - 没找到但有 legacy → [`LegacyHelperEngine`]（兼容老 Run Control 安装）
//!    - 都没有 → [`RealEngine`]（在 Tauri 进程内直接 spawn，需要 root）
//! 3. AppState 自己的 log/pid 路径走 **CyberRunner/**
//! 4. setup 里 spawn event pump（订阅 engine events → apply_event + emit）
//! 5. WindowEvent::CloseRequested → engine.shutdown() —— 干净退 helper

mod commands;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use cyber_runner_engine::{
    legacy_helper_available, BundledHelperEngine, Engine, LegacyHelperEngine, RealEngine,
};
use state::AppState;
use tauri::{Emitter, Manager, WindowEvent};

/// Cyber Runner 自己的 base dir —— log / pid / commands / helper.pid 都在这里。
///
/// 用 `dirs::data_local_dir()` 拿到平台标准位置：
/// - macOS:   `~/Library/Application Support/CyberRunner` （路径与 v1 完全相同，无迁移）
/// - Windows: `%LOCALAPPDATA%\CyberRunner` （= `~\AppData\Local\CyberRunner`）
/// - Linux:   `~/.local/share/CyberRunner`
const APP_BASE_LEAF: &str = "CyberRunner";
const APP_RUNTIME_LEAF: &str = "runtime";

fn default_base_dir() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_BASE_LEAF)
}

fn default_runtime_dir() -> PathBuf {
    default_base_dir().join(APP_RUNTIME_LEAF)
}

/// 找 cyber-runner-helper 二进制位置。
/// - dev：与当前可执行文件同目录（cargo target/debug/）
/// - release（.app）：兄弟目录 ../Resources/
fn find_helper_binary() -> Option<PathBuf> {
    let cur = std::env::current_exe().ok()?;
    let dir = cur.parent()?;

    // Cross-platform binary name —— Windows 上是 cyber-runner-helper.exe，
    // *nix 上是 cyber-runner-helper。EXE_SUFFIX 为 "" / ".exe"。
    let helper_name = format!("cyber-runner-helper{}", std::env::consts::EXE_SUFFIX);

    // dev：与 app 同目录（cargo target/debug/）。Windows 的 NSIS/MSI bundle 装到
    // `C:\Program Files\Cyber Runner\` 也是同目录，所以这一条就能 cover dev + win release。
    let dev_path = dir.join(&helper_name);
    if dev_path.exists() {
        return Some(dev_path);
    }

    // macOS .app release：Contents/MacOS/cyber-runner-app → Contents/Resources/cyber-runner-helper
    let bundle_path = dir.parent()?.join("Resources").join(&helper_name);
    if bundle_path.exists() {
        return Some(bundle_path);
    }

    None
}

/// 选最合适的引擎后端。
fn pick_engine(base_dir: &PathBuf) -> Arc<dyn Engine> {
    if let Some(helper) = find_helper_binary() {
        tracing::info!(
            ?helper,
            ?base_dir,
            "using BundledHelperEngine (own root helper)"
        );
        return Arc::new(BundledHelperEngine::new(helper, base_dir.clone())) as Arc<dyn Engine>;
    }
    if cfg!(target_os = "macos") && legacy_helper_available() {
        tracing::info!(
            "using LegacyHelperEngine — bridging via legacy RunControl LaunchDaemon \
             (bundled helper binary not found at expected path)"
        );
        return Arc::new(LegacyHelperEngine::new()) as Arc<dyn Engine>;
    }
    let runtime = default_runtime_dir();
    tracing::info!(
        ?runtime,
        "using RealEngine (direct python spawn, may need root)"
    );
    Arc::new(RealEngine::new(runtime)) as Arc<dyn Engine>
}

pub fn run() {
    let base_dir = default_base_dir();
    if let Err(err) = std::fs::create_dir_all(&base_dir) {
        tracing::warn!(?err, ?base_dir, "could not create CyberRunner base dir");
    }
    // Pre-create commands/ as the **user** (not root). Why: helper runs as
    // root and would otherwise create commands/ owned by root with mode 755,
    // making it unwritable for Tauri host (user) when dispatching requests
    // → "Permission denied (os error 13)". Creating it here means it's owned
    // by the user; root can still write into a user-owned dir, so helper
    // works fine. Helper's `create_dir_all` is idempotent → no conflict.
    let commands_dir = base_dir.join("commands");
    if let Err(err) = std::fs::create_dir_all(&commands_dir) {
        tracing::warn!(?err, ?commands_dir, "could not create commands dir");
    }
    let engine = pick_engine(&base_dir);
    let app_state = AppState::new_real(engine.clone(), base_dir);

    // 给 WindowEvent::CloseRequested 留一份 engine 句柄，关 App 时调 shutdown()
    let engine_for_shutdown = engine.clone();

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
        .on_window_event(move |_window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                tracing::info!("window close requested; shutting down engine");
                let engine = engine_for_shutdown.clone();
                // block on shutdown so we don't exit before helper is gone
                tauri::async_runtime::block_on(async move {
                    if let Err(err) = engine.shutdown().await {
                        tracing::warn!(?err, "engine shutdown failed");
                    }
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
