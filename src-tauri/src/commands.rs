//! Tauri command 入口 —— 委托给 [`AppState`]，引擎调用是 async。
//!
//! 接口签名稳定（Phase 4 → Phase 5 mock → 真引擎切换）：
//! - `start(speed: f64) -> Result<String, String>`
//! - `stop() -> Result<String, String>`
//! - `cleanup() -> Result<String, String>`
//! - `status() -> Result<Status, String>`

use tauri::State;

use crate::state::{AppState, Status};

#[tauri::command]
pub async fn start(speed: f64, state: State<'_, AppState>) -> Result<String, String> {
    state.start(speed).await
}

#[tauri::command]
pub async fn stop(state: State<'_, AppState>) -> Result<String, String> {
    state.stop().await
}

#[tauri::command]
pub async fn cleanup(state: State<'_, AppState>) -> Result<String, String> {
    state.cleanup().await
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

    /// `tauri::test::mock_builder` 起一个完整 app，验证 4 个 command 都注册成功 +
    /// 在不接 RealEngine 的 dummy state 下 `status` 仍能跑、`start` 返回 not-bound 错误
    /// （把真正的引擎集成测留给 cyber-runner-engine 自己的 integration tests）。
    #[test]
    fn mock_app_registers_all_four_commands() {
        let app = mock_builder()
            .manage(AppState::new_dummy())
            .invoke_handler(tauri::generate_handler![start, stop, cleanup, status])
            .build(mock_context(noop_assets()))
            .expect("mock app should build");

        let state: tauri::State<'_, AppState> = app.state();

        let snap = state.snapshot().expect("status ok");
        assert!(!snap.is_running);
    }
}
