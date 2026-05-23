//! 与 (未来的) helper / engine 的 JSON 协议，跨语言/跨进程共享。
//!
//! 字段命名走 `snake_case`，比 Swift 那版 `camelCase` 更符合 Rust + Python 习惯，
//! 反正这是全新协议，不需要兼容旧的 Swift 文件 IPC。

use serde::{Deserialize, Serialize};

pub const DEFAULT_ROUTE: &str = "more_route/xitucheng/xitucheng_1.txt";
pub const MIN_SPEED_MPS: f64 = 0.5;
pub const MAX_SPEED_MPS: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Start,
    Stop,
    Cleanup,
    /// 让 helper 主循环优雅退出（写 response + break loop）。
    /// 由 Tauri host 在 App 退出时发出，避免在用户机上留下常驻 root 进程。
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub action: Action,
    pub log_path: String,
    pub pid_path: String,
    pub route_config: String,
    pub speed_meters_per_second: Option<f64>,
}

impl Request {
    pub fn new_start(
        id: impl Into<String>,
        log_path: impl Into<String>,
        pid_path: impl Into<String>,
        route_config: impl Into<String>,
        speed: f64,
    ) -> Self {
        Self {
            id: id.into(),
            action: Action::Start,
            log_path: log_path.into(),
            pid_path: pid_path.into(),
            route_config: route_config.into(),
            speed_meters_per_second: Some(clamp_speed(speed)),
        }
    }

    pub fn new_stop(
        id: impl Into<String>,
        log_path: impl Into<String>,
        pid_path: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            action: Action::Stop,
            log_path: log_path.into(),
            pid_path: pid_path.into(),
            route_config: DEFAULT_ROUTE.into(),
            speed_meters_per_second: None,
        }
    }

    pub fn new_cleanup(
        id: impl Into<String>,
        log_path: impl Into<String>,
        pid_path: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            action: Action::Cleanup,
            log_path: log_path.into(),
            pid_path: pid_path.into(),
            route_config: DEFAULT_ROUTE.into(),
            speed_meters_per_second: None,
        }
    }

    /// "请 helper 优雅退出" 请求 —— Tauri host 在 App 关闭时发。
    /// log/pid 路径不相关，留空字符串。
    pub fn new_shutdown(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            action: Action::Shutdown,
            log_path: String::new(),
            pid_path: String::new(),
            route_config: DEFAULT_ROUTE.into(),
            speed_meters_per_second: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub status: i32,
    pub output: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Response {
    pub fn ok(id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: 0,
            output: output.into(),
            updated_at: chrono::Utc::now(),
        }
    }

    pub fn fail(id: impl Into<String>, status: i32, output: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status,
            output: output.into(),
            updated_at: chrono::Utc::now(),
        }
    }
}

pub fn clamp_speed(value: f64) -> f64 {
    value.clamp(MIN_SPEED_MPS, MAX_SPEED_MPS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_request_carries_speed_and_route() {
        let req = Request::new_start("abc", "/tmp/l", "/tmp/p", "routes/x.txt", 4.7);
        assert_eq!(req.id, "abc");
        assert_eq!(req.action, Action::Start);
        assert_eq!(req.route_config, "routes/x.txt");
        assert_eq!(req.speed_meters_per_second, Some(4.7));
    }

    #[test]
    fn stop_request_has_no_speed() {
        let req = Request::new_stop("s1", "/tmp/l", "/tmp/p");
        assert_eq!(req.action, Action::Stop);
        assert!(req.speed_meters_per_second.is_none());
        assert_eq!(req.route_config, DEFAULT_ROUTE);
    }

    #[test]
    fn cleanup_request_has_no_speed() {
        let req = Request::new_cleanup("c1", "/tmp/l", "/tmp/p");
        assert_eq!(req.action, Action::Cleanup);
        assert!(req.speed_meters_per_second.is_none());
    }

    #[test]
    fn clamp_speed_lower_bound() {
        assert_eq!(clamp_speed(0.1), MIN_SPEED_MPS);
    }

    #[test]
    fn clamp_speed_upper_bound() {
        assert_eq!(clamp_speed(99.9), MAX_SPEED_MPS);
    }

    #[test]
    fn clamp_speed_middle_value() {
        assert_eq!(clamp_speed(4.7), 4.7);
    }

    #[test]
    fn request_round_trips_through_json() {
        let req = Request::new_start("r1", "/l", "/p", "routes/x.txt", 3.0);
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn action_serializes_as_snake_case() {
        let json = serde_json::to_string(&Action::Start).unwrap();
        assert_eq!(json, "\"start\"");
        let json = serde_json::to_string(&Action::Stop).unwrap();
        assert_eq!(json, "\"stop\"");
        let json = serde_json::to_string(&Action::Cleanup).unwrap();
        assert_eq!(json, "\"cleanup\"");
    }

    #[test]
    fn response_ok_has_zero_status() {
        let resp = Response::ok("r1", "started");
        assert_eq!(resp.status, 0);
        assert_eq!(resp.output, "started");
    }

    #[test]
    fn response_fail_carries_status() {
        let resp = Response::fail("r2", 1, "boom");
        assert_eq!(resp.status, 1);
        assert_eq!(resp.output, "boom");
    }

    #[test]
    fn start_speed_clamped_at_construction() {
        let req = Request::new_start("x", "/l", "/p", "r", 100.0);
        assert_eq!(req.speed_meters_per_second, Some(MAX_SPEED_MPS));
        let req = Request::new_start("y", "/l", "/p", "r", 0.0);
        assert_eq!(req.speed_meters_per_second, Some(MIN_SPEED_MPS));
    }
}
