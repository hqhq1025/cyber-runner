//! Tauri commands. Thin: validation + delegation to the engine / stores.

use std::path::PathBuf;

use cyber_runner_core::Datum;
use cyber_runner_engine::{DeviceList, RunPlan, Snapshot};
use serde::Serialize;
use tauri::State;

use crate::routes::{RouteDraft, RouteStore, RouteView};
use crate::settings::Settings;
use crate::AppState;

type CmdResult<T> = Result<T, String>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub version: &'static str,
    pub settings: Settings,
    pub routes: Vec<RouteView>,
    pub snapshot: Snapshot,
    pub devices: DeviceList,
    pub logs: Vec<cyber_runner_engine::LogLine>,
}

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Bootstrap {
    Bootstrap {
        version: env!("CARGO_PKG_VERSION"),
        settings: state.settings.lock().unwrap().clone(),
        routes: state.routes.list(),
        snapshot: state.engine.snapshot(),
        devices: state.engine.devices().borrow().clone(),
        logs: state.log_history(),
    }
}

/// Persist settings; while running, speed / realism / goal apply live.
#[tauri::command]
pub fn save_settings(settings: Settings, state: State<'_, AppState>) -> CmdResult<Settings> {
    let s = settings.sanitized();
    let prev = std::mem::replace(&mut *state.settings.lock().unwrap(), s.clone());
    state
        .settings_file
        .save(&s)
        .map_err(|e| format!("保存设置失败：{e}"))?;
    let e = &state.engine;
    if prev.speed_mps != s.speed_mps {
        let _ = e.set_speed(s.speed_mps);
    }
    if prev.realism != s.realism {
        let _ = e.set_variation(s.realism.variation());
    }
    if prev.stop_after() != s.stop_after() || prev.on_finish != s.on_finish {
        let _ = e.set_stop_after(s.stop_after(), s.on_finish);
    }
    Ok(s)
}

#[tauri::command]
pub fn start(state: State<'_, AppState>) -> CmdResult<()> {
    let s = state.settings.lock().unwrap().clone();
    let (name, points) = state
        .routes
        .points(&s.route_id)
        .ok_or("选中的路线不存在了，请重新选择")?;
    // If the preferred device isn't attached, fall back to whatever is.
    let attached = state.engine.devices().borrow().devices.clone();
    let udid = s
        .udid
        .clone()
        .filter(|u| attached.iter().any(|d| &d.udid == u));
    state
        .engine
        .start(RunPlan {
            udid,
            route_name: name,
            points,
            loop_mode: s.loop_mode,
            speed_mps: s.speed_mps,
            variation: s.realism.variation(),
            stop_after: s.stop_after(),
            on_finish: s.on_finish,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pause(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.pause().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn resume(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.resume().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stop(state: State<'_, AppState>) -> CmdResult<()> {
    state.engine.stop().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restore_location(state: State<'_, AppState>) -> CmdResult<()> {
    let udid = state.settings.lock().unwrap().udid.clone();
    let attached = state.engine.devices().borrow().devices.clone();
    let udid = udid.filter(|u| attached.iter().any(|d| &d.udid == u));
    state
        .engine
        .restore_location(udid)
        .await
        .map_err(|e| format!("{e}。{}", e.hint()))
}

#[tauri::command]
pub fn list_routes(state: State<'_, AppState>) -> Vec<RouteView> {
    state.routes.list()
}

#[tauri::command]
pub fn read_route_file(path: PathBuf) -> CmdResult<RouteDraft> {
    RouteStore::read_file(&path)
}

#[tauri::command]
pub fn preview_route(text: String, datum: Datum) -> CmdResult<RouteView> {
    RouteStore::preview(&text, datum)
}

#[tauri::command]
pub fn create_route(
    name: String,
    text: String,
    datum: Datum,
    state: State<'_, AppState>,
) -> CmdResult<RouteView> {
    state.routes.create(&name, &text, datum)
}

#[tauri::command]
pub fn rename_route(id: String, name: String, state: State<'_, AppState>) -> CmdResult<()> {
    state.routes.rename(&id, &name)
}

#[tauri::command]
pub fn delete_route(id: String, state: State<'_, AppState>) -> CmdResult<()> {
    state.routes.delete(&id)
}
