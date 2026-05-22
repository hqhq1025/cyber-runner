//! Tauri command 入口 —— mock 阶段全部委托给 `AppState`。
//!
//! 接口签名与未来真接 RealEngine 时保持一致：
//! - `start(speed: f64) -> Result<String, String>`
//! - `stop() -> Result<String, String>`
//! - `cleanup() -> Result<String, String>`
//! - `status() -> Result<Status, String>`
//!
//! 之后只需要把 `AppState::*` 内部从 `MockState` 切到真实 engine handle。

use tauri::State;

use crate::state::{AppState, Status};

#[tauri::command]
pub async fn start(speed: f64, state: State<'_, AppState>) -> Result<String, String> {
    state.start(speed)
}

#[tauri::command]
pub async fn stop(state: State<'_, AppState>) -> Result<String, String> {
    state.stop()
}

#[tauri::command]
pub async fn cleanup(state: State<'_, AppState>) -> Result<String, String> {
    state.cleanup()
}

#[tauri::command]
pub async fn status(state: State<'_, AppState>) -> Result<Status, String> {
    state.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::Manager;

    /// 用 `tauri::test::mock_builder` 起一个完整的 App，验证 4 个 command 都注册成功、
    /// state 也挂上去，并且通过 Manager 能拿回 state 调到 happy path —— 不应 panic。
    #[test]
    fn mock_app_registers_all_four_commands() {
        let app = mock_builder()
            .manage(AppState::default())
            .invoke_handler(tauri::generate_handler![start, stop, cleanup, status])
            .build(mock_context(noop_assets()))
            .expect("mock app should build");

        let state: tauri::State<'_, AppState> = app.state();

        // status 在初始空状态下应该返回 Ok
        let snap = state.snapshot().expect("status ok");
        assert!(!snap.is_running);

        // 直接驱动 mock 状态机走一遍 start → status → stop → cleanup
        state.start(4.7).expect("start ok");
        let snap = state.snapshot().expect("status ok");
        assert!(snap.is_running);
        assert_eq!(snap.speed_mps, 4.7);

        state.stop().expect("stop ok");
        let snap = state.snapshot().expect("status ok after stop");
        assert!(!snap.is_running);

        state.cleanup().expect("cleanup ok");
        let snap = state.snapshot().expect("status ok after cleanup");
        assert!(snap.recent_log.iter().any(|l| l.contains("cleanup")));
    }
}
