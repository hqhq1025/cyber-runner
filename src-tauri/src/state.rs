//! 真后端：包一层 `AppState` 把 [`cyber_runner_engine::RealEngine`] 暴露给 Tauri commands。
//!
//! 关键约束：commands 层签名（`start(speed) -> Result<String,String>` 等）不变，
//! 仅替换 `AppState` 内部 —— Phase 4 留下的 `MockState` 退役。
//!
//! 状态来源：
//! - `is_running` / `recent_log` / `last_error` 由后台 event pump 从
//!   `RealEngine::subscribe()` 拉到的 `Event` 流驱动
//! - `speed_mps` / `started_at` 由 `start()` 直接写入
//! - `Status::server_now_rfc3339` 用 `Utc::now()` 当场算

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use cyber_runner_core::{clamp_speed, pace_text, Request};
use cyber_runner_engine::{Engine, Event};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 保留的日志行上限，避免 UI 撑爆。
const RECENT_LOG_CAPACITY: usize = 200;
/// 默认倒计时（分钟），对齐 Swift `RunControlModel.countdownMinutes`。
const DEFAULT_COUNTDOWN_MINUTES: i64 = 30;

/// 前端拉的状态快照（`camelCase` 配合 TypeScript 习惯）。
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
    pub server_now_rfc3339: String,
}

/// 内部状态 —— 受 `Mutex` 保护，被 event pump 和 commands 同时写。
#[derive(Debug, Clone)]
struct InnerState {
    is_running: bool,
    speed_mps: f64,
    started_at: Option<DateTime<Utc>>,
    countdown_total_seconds: i64,
    recent_log: VecDeque<String>,
    last_error: Option<String>,
}

impl Default for InnerState {
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

impl InnerState {
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

/// AppState 对外接口。生产 (`new_real`) 持任意 `Engine` 实现（macOS 默认走
/// `LegacyHelperEngine`，其他平台走 `RealEngine`）；测试 (`new_dummy`)
/// 不绑引擎，便于单测事件 → InnerState 的状态机。
pub struct AppState {
    inner: Mutex<InnerState>,
    engine: Option<Arc<dyn Engine>>,
    log_path: String,
    pid_path: String,
}

impl AppState {
    /// 生产构造：绑一个共享的引擎实现（trait object）。
    /// `base_dir` 通常是 `~/Library/Application Support/RunControl`（沿用
    /// legacy Run Control 的 log/pid 路径）。
    pub fn new_real(engine: Arc<dyn Engine>, base_dir: PathBuf) -> Self {
        let log_path = base_dir.join("run.log").to_string_lossy().into_owned();
        let pid_path = base_dir.join("run.pid").to_string_lossy().into_owned();
        Self {
            inner: Mutex::new(InnerState::default()),
            engine: Some(engine),
            log_path,
            pid_path,
        }
    }

    /// 测试构造：不接引擎，commands 路径不可用，只能驱动事件状态机。
    #[cfg(test)]
    pub fn new_dummy() -> Self {
        Self {
            inner: Mutex::new(InnerState::default()),
            engine: None,
            log_path: "/tmp/cyber-runner-test.log".into(),
            pid_path: "/tmp/cyber-runner-test.pid".into(),
        }
    }

    /// 让 `RealEngine` 跑 start。引擎事件后续由 pump 写回 InnerState。
    pub async fn start(&self, speed: f64) -> Result<String, String> {
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| "engine not bound (test build)".to_string())?;
        {
            let mut s = self.lock()?;
            if s.is_running {
                return Err("already running".into());
            }
            let clamped = clamp_speed(speed);
            s.speed_mps = clamped;
            s.last_error = None;
            // 乐观先记一条 UI 反馈；真正的 Started 由 event pump 翻成 is_running=true
            s.push_log(format!("requesting start at {clamped:.1} m/s"));
        }
        let req = Request::new_start(
            Uuid::new_v4().to_string(),
            &self.log_path,
            &self.pid_path,
            cyber_runner_core::DEFAULT_ROUTE,
            speed,
        );
        match engine.start(&req).await {
            Ok(resp) => Ok(resp.output),
            Err(e) => {
                let msg = e.to_string();
                if let Ok(mut s) = self.inner.lock() {
                    s.last_error = Some(msg.clone());
                    s.push_log(format!("error: {msg}"));
                }
                Err(msg)
            }
        }
    }

    pub async fn stop(&self) -> Result<String, String> {
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| "engine not bound (test build)".to_string())?;
        let req = Request::new_stop(Uuid::new_v4().to_string(), &self.log_path, &self.pid_path);
        match engine.stop(&req).await {
            Ok(resp) => Ok(resp.output),
            Err(e) => {
                let msg = e.to_string();
                if let Ok(mut s) = self.inner.lock() {
                    s.last_error = Some(msg.clone());
                }
                Err(msg)
            }
        }
    }

    pub async fn cleanup(&self) -> Result<String, String> {
        let engine = self
            .engine
            .as_ref()
            .ok_or_else(|| "engine not bound (test build)".to_string())?;
        let req = Request::new_cleanup(Uuid::new_v4().to_string(), &self.log_path, &self.pid_path);
        match engine.cleanup(&req).await {
            Ok(resp) => Ok(resp.output),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn snapshot(&self) -> Result<Status, String> {
        let s = self.lock()?;
        Ok(s.snapshot_at(Utc::now()))
    }

    /// Event pump 调这个，把引擎事件折叠进 InnerState。
    /// 公开是为了让 setup 里的 background task 能调（无需保护可见性）。
    pub fn apply_event(&self, event: &Event) {
        let Ok(mut s) = self.inner.lock() else {
            return;
        };
        match event {
            Event::Log(line) => s.push_log(line),
            Event::Started { pid } => {
                s.is_running = true;
                s.started_at = Some(Utc::now());
                s.last_error = None;
                s.push_log(format!("started (pid={pid})"));
            }
            Event::Stopped { exit_code } => {
                s.is_running = false;
                s.started_at = None;
                s.push_log(format!("stopped (exit_code={exit_code:?})"));
            }
            Event::Error(msg) => {
                s.last_error = Some(msg.clone());
                s.push_log(format!("engine error: {msg}"));
            }
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, InnerState>, String> {
        self.inner
            .lock()
            .map_err(|e| format!("state mutex poisoned: {e}"))
    }
}

/// 测试/setup 用，暴露速度边界。
#[allow(dead_code)]
pub const SPEED_BOUNDS: (f64, f64) = (cyber_runner_core::MIN_SPEED_MPS, cyber_runner_core::MAX_SPEED_MPS);

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy() -> AppState {
        AppState::new_dummy()
    }

    #[test]
    fn default_snapshot_is_idle() {
        let app = dummy();
        let snap = app.snapshot().unwrap();
        assert!(!snap.is_running);
        assert_eq!(snap.elapsed_seconds, 0);
        assert!(snap.remaining_seconds > 0);
        assert!(snap.recent_log.is_empty());
    }

    #[test]
    fn started_event_marks_running_and_records_pid() {
        let app = dummy();
        app.apply_event(&Event::Started { pid: 12345 });
        let snap = app.snapshot().unwrap();
        assert!(snap.is_running);
        assert!(snap.recent_log.iter().any(|l| l.contains("pid=12345")));
    }

    #[test]
    fn stopped_event_clears_running() {
        let app = dummy();
        app.apply_event(&Event::Started { pid: 1 });
        app.apply_event(&Event::Stopped { exit_code: Some(0) });
        let snap = app.snapshot().unwrap();
        assert!(!snap.is_running);
        assert_eq!(snap.elapsed_seconds, 0);
    }

    #[test]
    fn log_event_appends_with_timestamp() {
        let app = dummy();
        app.apply_event(&Event::Log("hello world".into()));
        let snap = app.snapshot().unwrap();
        assert_eq!(snap.recent_log.len(), 1);
        assert!(snap.recent_log[0].contains("hello world"));
        assert!(snap.recent_log[0].starts_with('['));
    }

    #[test]
    fn error_event_sets_last_error() {
        let app = dummy();
        app.apply_event(&Event::Error("boom".into()));
        let snap = app.snapshot().unwrap();
        assert_eq!(snap.last_error.as_deref(), Some("boom"));
    }

    #[test]
    fn log_buffer_caps_at_capacity() {
        let app = dummy();
        for i in 0..(RECENT_LOG_CAPACITY + 50) {
            app.apply_event(&Event::Log(format!("line {i}")));
        }
        let snap = app.snapshot().unwrap();
        assert_eq!(snap.recent_log.len(), RECENT_LOG_CAPACITY);
        // first preserved line should be line 50 (50 oldest dropped)
        assert!(snap.recent_log[0].contains("line 50"));
    }

    #[test]
    fn snapshot_carries_pace_text() {
        let app = dummy();
        let snap = app.snapshot().unwrap();
        assert!(snap.pace_text.ends_with("/km"));
    }

    #[tokio::test]
    async fn start_without_engine_returns_error() {
        let app = dummy();
        let result = app.start(4.7).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not bound"));
    }
}
