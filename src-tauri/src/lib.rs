//! Cyber Runner desktop host.
//!
//! One process, no privileges: the engine thread talks to the phone, this
//! layer forwards its state to the window as events and persists settings
//! and imported routes.

mod commands;
mod routes;
mod settings;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cyber_runner_engine::{Engine, EngineConfig, LogLine, Phase};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, RunEvent};
use tauri_plugin_notification::NotificationExt;

use routes::RouteStore;
use settings::{Settings, SettingsFile};

const LOG_HISTORY: usize = 300;

pub struct AppState {
    engine: Engine,
    routes: RouteStore,
    settings: Mutex<Settings>,
    settings_file: SettingsFile,
    logs: Arc<Mutex<VecDeque<LogLine>>>,
}

impl AppState {
    fn log_history(&self) -> Vec<LogLine> {
        self.logs.lock().unwrap().iter().cloned().collect()
    }
}

/// Present while the phone's location is simulated by us. If the app dies
/// without restoring (force quit, Dock → Quit on macOS which bypasses
/// Tauri's exit events, power loss), the next launch restores it.
const MARKER: &str = "simulating.json";

#[derive(serde::Serialize, serde::Deserialize)]
struct Marker {
    udid: Option<String>,
}

/// Stop the run (restoring the real location) and quit.
fn graceful_quit(app: &AppHandle) {
    let engine = app.state::<AppState>().engine.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = tokio::time::timeout(Duration::from_secs(10), engine.stop()).await;
        app.exit(0);
    });
}

/// Our own menu: the predefined macOS "Quit" item terminates the process
/// directly and skips cleanup, so Cmd+Q is routed through `graceful_quit`.
fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let quit = MenuItem::with_id(app, "quit", "退出 Cyber Runner", true, Some("CmdOrCtrl+Q"))?;
    let app_menu = Submenu::with_items(
        app,
        "Cyber Runner",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("关于 Cyber Runner"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "编辑",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "窗口",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &edit, &window])
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("CyberRunner")
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,idevice=warn,jktcp=warn".into()),
        )
        .init();

    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let settings_file = SettingsFile::new(dir.join("settings.json"));
    let settings = settings_file.load();
    let engine = Engine::spawn(EngineConfig {
        pair_dir: dir.join("pairing"),
        ddi_cache_dir: dir.join("ddi"),
        ddi_extra_dir: settings.ddi_dir.clone().map(PathBuf::from),
    });
    let state = AppState {
        engine: engine.clone(),
        routes: RouteStore::new(dir.join("routes")),
        settings: Mutex::new(settings),
        settings_file,
        logs: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_HISTORY))),
    };
    let logs = state.logs.clone();
    let marker_path = dir.join(MARKER);
    let marker_w = marker_path.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            // Two instances would fight over the phone; focus the first.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::save_settings,
            commands::start,
            commands::pause,
            commands::resume,
            commands::stop,
            commands::restore_location,
            commands::list_routes,
            commands::read_route_file,
            commands::preview_route,
            commands::create_route,
            commands::rename_route,
            commands::delete_route,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            if cfg!(target_os = "macos") {
                app.set_menu(build_menu(&handle)?)?;
                app.on_menu_event(|app, e| {
                    if e.id() == "quit" {
                        graceful_quit(app);
                    }
                });
            }

            // Crash recovery: a marker left behind means the phone may still
            // be at a simulated location. Restore as soon as it is attached.
            let marker = marker_path.clone();
            if let Some(m) = std::fs::read(&marker)
                .ok()
                .and_then(|b| serde_json::from_slice::<Marker>(&b).ok())
            {
                let engine = engine.clone();
                let mut devices = engine.devices();
                tauri::async_runtime::spawn(async move {
                    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
                    loop {
                        let present = {
                            let d = devices.borrow_and_update();
                            match &m.udid {
                                Some(u) => d.devices.iter().any(|x| &x.udid == u),
                                None => !d.devices.is_empty(),
                            }
                        };
                        if present {
                            tracing::info!(
                                "previous session left the location simulated; restoring"
                            );
                            if engine.restore_location(m.udid.clone()).await.is_ok() {
                                let _ = std::fs::remove_file(&marker);
                            }
                            break;
                        }
                        if tokio::time::timeout_at(deadline, devices.changed())
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });
            }

            // Engine snapshot → window (the watch channel coalesces bursts).
            let mut snaps = engine.subscribe();
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                let mut last_phase = Phase::Idle;
                let mut marked = false;
                while snaps.changed().await.is_ok() {
                    let s = snaps.borrow_and_update().clone();
                    if s.simulating != marked {
                        marked = s.simulating;
                        if marked {
                            let m = Marker {
                                udid: s.device.as_ref().map(|d| d.udid.clone()),
                            };
                            let _ = std::fs::write(
                                &marker_w,
                                serde_json::to_vec(&m).unwrap_or_default(),
                            );
                        } else {
                            let _ = std::fs::remove_file(&marker_w);
                        }
                    }
                    if s.phase == Phase::Finished && last_phase != Phase::Finished {
                        let _ = h
                            .notification()
                            .builder()
                            .title("Cyber Runner · 跑完了")
                            .body(format!(
                                "已跑 {:.2} km，用时 {}。先在校园跑 App 里结束跑步，再回来恢复真实定位。",
                                s.distance_m / 1000.0,
                                cyber_runner_core::format::duration_text(s.moving_s)
                            ))
                            .sound("Glass")
                            .show();
                        if let Some(w) = h.get_webview_window("main") {
                            let _ = w.request_user_attention(Some(
                                tauri::UserAttentionType::Critical,
                            ));
                        }
                    }
                    last_phase = s.phase;
                    let _ = h.emit("snapshot", &s);
                    // Cap UI updates at ~10 Hz.
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            });
            let mut devices = engine.devices();
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                while devices.changed().await.is_ok() {
                    let d = devices.borrow_and_update().clone();
                    let _ = h.emit("devices", &d);
                }
            });
            let mut log_rx = engine.logs();
            let h = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match log_rx.recv().await {
                        Ok(line) => {
                            {
                                let mut l = logs.lock().unwrap();
                                if l.len() == LOG_HISTORY {
                                    l.pop_front();
                                }
                                l.push_back(line.clone());
                            }
                            let _ = h.emit("log", &line);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|handle, event| {
        if let RunEvent::ExitRequested { .. } = event {
            // Always give the phone its real location back before quitting.
            // The engine lives on its own thread, so blocking here is safe.
            let state = handle.state::<AppState>();
            if state.engine.snapshot().simulating
                || !matches!(state.engine.snapshot().phase, Phase::Idle)
            {
                let engine = state.engine.clone();
                tauri::async_runtime::block_on(async move {
                    let _ = tokio::time::timeout(Duration::from_secs(10), engine.stop()).await;
                });
            }
        }
    });
}
