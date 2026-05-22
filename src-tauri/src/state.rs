//! Mock 后端状态 —— Phase 4 用，等 RealEngine 接通后会被替换。
//!
//! 设计原则：所有真正的业务逻辑都放在 `MockState` / `AppState` 的普通方法里，
//! `#[tauri::command]` 包装层（`commands.rs`）只做 `Result` 映射。
//! 这样核心逻辑可以脱离 Tauri 直接单测，到时候接真引擎时也只换 body。

use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use fake_run_core::{clamp_speed, pace_text};
use serde::{Deserialize, Serialize};

/// 最多保留的日志行数，避免无限增长撑爆 UI。
const RECENT_LOG_CAPACITY: usize = 200;
/// Mock 倒计时默认值（30 分钟），对齐 Swift `RunControlModel.countdownMinutes`。
const DEFAULT_COUNTDOWN_MINUTES: i64 = 30;

/// 暴露给前端的状态快照。字段命名走 `camelCase` 以匹配 JS/TS 惯例。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub is_running: bool,
    pub elapsed_seconds: i64,
    pub remaining_seconds: i64,
    pub speed_mps: f64,
    pub pace_text: String,
    pub recent_log: Vec<String>,
    pub last_error: Option<String>,
    /// 服务端"现在"，方便前端展示同步时钟（不依赖客户端时间）。
    pub server_now_rfc3339: String,
}

/// 内部 mock 状态，被 `AppState` 用 `Mutex` 包起来。
#[derive(Debug, Clone)]
pub struct MockState {
    pub is_running: bool,
    pub speed_mps: f64,
    pub started_at: Option<DateTime<Utc>>,
    pub countdown_total_seconds: i64,
    pub recent_log: VecDeque<String>,
    pub last_error: Option<String>,
}

impl Default for MockState {
    fn default() -> Self {
        Self {
            is_running: false,
            speed_mps: 4.7,
            started_at: None,
            countdown_total_seconds: DEFAULT_COUNTDOWN_MINUTES * 60,
            recent_log: VecDeque::with_capacity(RECENT_LOG_CAPACITY),
            last_error: None,
        }
    }
}

impl MockState {
    fn push_log(&mut self, line: impl Into<String>) {
        if self.recent_log.len() == RECENT_LOG_CAPACITY {
            self.recent_log.pop_front();
        }
        let line = line.into();
        let ts = Utc::now().format("%H:%M:%S");
        self.recent_log.push_back(format!("[{ts}] {line}"));
    }

    fn snapshot_at(&self, now: DateTime<Utc>) -> Status {
        let elapsed = self
            .started_at
            .map(|t| (now - t).num_seconds().max(0))
            .unwrap_or(0);
        let remaining = if self.is_running {
            (self.countdown_total_seconds - elapsed).max(0)
        } else {
            self.countdown_total_seconds
        };
        Status {
            is_running: self.is_running,
            elapsed_seconds: elapsed,
            remaining_seconds: remaining,
            speed_mps: self.speed_mps,
            pace_text: pace_text(self.speed_mps),
            recent_log: self.recent_log.iter().cloned().collect(),
            last_error: self.last_error.clone(),
            server_now_rfc3339: now.to_rfc3339(),
        }
    }
}

/// 与 Tauri 集成的容器。前端通过 `tauri::State<'_, AppState>` 访问。
pub struct AppState {
    inner: Mutex<MockState>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(MockState::default()),
        }
    }
}

impl AppState {
    /// 启动 mock 引擎。speed 会被钳制到 `[MIN_SPEED_MPS, MAX_SPEED_MPS]`。
    /// 已经运行中再次调用返回 `Err`，避免静默吃掉操作。
    pub fn start(&self, speed: f64) -> Result<String, String> {
        let mut s = self.lock()?;
        if s.is_running {
            return Err("already running".into());
        }
        let clamped = clamp_speed(speed);
        s.speed_mps = clamped;
        s.started_at = Some(Utc::now());
        s.is_running = true;
        s.last_error = None;
        s.push_log(format!("[mock] start speed={clamped:.1} m/s"));
        Ok("started".into())
    }

    /// 停止。未运行也允许调用（幂等），方便 UI 重置。
    pub fn stop(&self) -> Result<String, String> {
        let mut s = self.lock()?;
        if s.is_running {
            s.push_log("[mock] stop");
        }
        s.is_running = false;
        s.started_at = None;
        Ok("stopped".into())
    }

    /// 清理：重置状态 + 清空日志/错误。等价于 Swift 里的 cleanup helper。
    pub fn cleanup(&self) -> Result<String, String> {
        let mut s = self.lock()?;
        s.is_running = false;
        s.started_at = None;
        s.last_error = None;
        s.recent_log.clear();
        s.push_log("[mock] cleanup ok");
        Ok("cleaned".into())
    }

    /// 读取最新快照，前端轮询。
    pub fn snapshot(&self) -> Result<Status, String> {
        let s = self.lock()?;
        Ok(s.snapshot_at(Utc::now()))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, MockState>, String> {
        self.inner
            .lock()
            .map_err(|e| format!("state mutex poisoned: {e}"))
    }
}

/// 重新导出速度边界，方便 commands.rs / 调用方判断。
#[allow(dead_code)]
pub const SPEED_BOUNDS: (f64, f64) = (fake_run_core::MIN_SPEED_MPS, fake_run_core::MAX_SPEED_MPS);

#[cfg(test)]
mod tests {
    use super::*;
    use fake_run_core::{duration_text, MAX_SPEED_MPS};

    #[test]
    fn default_state_is_idle() {
        let app = AppState::default();
        let snap = app.snapshot().unwrap();
        assert!(!snap.is_running);
        assert_eq!(snap.elapsed_seconds, 0);
        assert!(snap.remaining_seconds > 0);
        assert_eq!(snap.recent_log.len(), 0);
        assert!(snap.last_error.is_none());
    }

    #[test]
    fn start_marks_running_and_clamps_speed() {
        let app = AppState::default();
        app.start(99.0).unwrap();
        let snap = app.snapshot().unwrap();
        assert!(snap.is_running);
        assert_eq!(snap.speed_mps, MAX_SPEED_MPS);
        assert!(snap.recent_log.iter().any(|l| l.contains("start")));
    }

    #[test]
    fn double_start_is_rejected() {
        let app = AppState::default();
        app.start(4.0).unwrap();
        assert!(app.start(4.0).is_err());
    }

    #[test]
    fn stop_is_idempotent() {
        let app = AppState::default();
        app.stop().unwrap();
        app.start(2.0).unwrap();
        app.stop().unwrap();
        app.stop().unwrap();
        let snap = app.snapshot().unwrap();
        assert!(!snap.is_running);
    }

    #[test]
    fn cleanup_clears_log_and_state() {
        let app = AppState::default();
        app.start(3.0).unwrap();
        app.cleanup().unwrap();
        let snap = app.snapshot().unwrap();
        assert!(!snap.is_running);
        // cleanup 后会推一条 "cleanup ok" 日志，所以恰好 1 行
        assert_eq!(snap.recent_log.len(), 1);
    }

    #[test]
    fn snapshot_carries_pace_text() {
        let app = AppState::default();
        let snap = app.snapshot().unwrap();
        assert!(snap.pace_text.ends_with("/km"));
    }

    #[test]
    fn duration_helper_round_trips() {
        // 用一下 fake_run_core 的 duration_text，确保依赖图没断
        assert_eq!(duration_text(65), "01:05");
    }
}
