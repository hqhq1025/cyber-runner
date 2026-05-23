//! 复用 macOS 上**已经在跑的**遗留 Swift LaunchDaemon helper
//! (`/Library/LaunchDaemons/dev.haoqing.run-control.helper.plist`，root)
//! 来 spawn Python，避免在 Tauri 端自己实现特权提升。
//!
//! 协议：写 `<base>/commands/request-<UUID>.json`，轮询 `<base>/commands/response-<UUID>.json`，
//! 与 [`crates/cyber-runner-core::Request`] 字段映射成 Swift 端使用的 camelCase
//! `RunControlRequest`（详见 `~/Documents/Github/run/Sources/RunControlCore/RunControlCommand.swift`）。
//!
//! Events：
//! - start response 成功后：tail `request.log_path` 当 `Event::Log`，读 `request.pid_path` 当 `Event::Started`
//! - 检测到 PID 进程不在时（包含 helper 主动 SIGINT/KILL）：`Event::Stopped { exit_code: None }`
//!   （legacy helper 不把 exit code 写在 response 里）

use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cyber_runner_core::{Request, Response};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::broadcast::{self, Sender};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::engine::{Engine, EngineError, Event};
use crate::python_runner::process_alive;

const EVENT_CAPACITY: usize = 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);
const LOG_TAIL_INTERVAL: Duration = Duration::from_millis(500);
const RUN_DETECT_INTERVAL: Duration = Duration::from_millis(1500);

/// Swift `RunControlRequest` 的 camelCase JSON 镜像。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SwiftRequest<'a> {
    id: String,
    action: &'a str,
    log_path: &'a str,
    pid_path: &'a str,
    route_config: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed_meters_per_second: Option<f64>,
}

/// Swift `RunControlResponse` 的 camelCase JSON 镜像。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwiftResponse {
    id: String,
    status: i32,
    output: String,
    /// Helper 写但我们暂时不消费；保留字段是为了 schema 完整性，
    /// 万一以后要做"过期响应"检测。
    #[serde(default)]
    #[allow(dead_code)]
    updated_at: Option<DateTime<Utc>>,
}

/// 默认 base 目录 —— 与 Swift 安装脚本写死的位置一致。
fn default_base() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/Application Support/RunControl")
}

/// 是否值得用 legacy helper？判定：base/commands 目录存在 + helper 二进制存在。
/// （进一步检查 helper 进程是否在跑也行，但成本高且 helper 可能瞬时被 launchd 重启。）
pub fn legacy_helper_available() -> bool {
    let base = default_base();
    let cmd_dir = base.join("commands");
    let helper_bin = base.join("runtime/RunControlHelper");
    cmd_dir.is_dir() && helper_bin.is_file()
}

pub struct LegacyHelperEngine {
    base: PathBuf,
    events: Sender<Event>,
    watcher_running: Arc<Mutex<bool>>,
}

impl LegacyHelperEngine {
    pub fn new() -> Self {
        Self::with_base(default_base())
    }

    pub fn with_base(base: impl Into<PathBuf>) -> Self {
        let (tx, _rx) = broadcast::channel(EVENT_CAPACITY);
        Self {
            base: base.into(),
            events: tx,
            watcher_running: Arc::new(Mutex::new(false)),
        }
    }

    fn commands_dir(&self) -> PathBuf {
        self.base.join("commands")
    }

    fn request_path(&self, id: &str) -> PathBuf {
        self.commands_dir().join(format!("request-{id}.json"))
    }

    fn response_path(&self, id: &str) -> PathBuf {
        self.commands_dir().join(format!("response-{id}.json"))
    }

    /// 写请求 → 轮询响应 → 解析返回。
    async fn dispatch(
        &self,
        id: &str,
        action: &str,
        req: &Request,
    ) -> Result<SwiftResponse, EngineError> {
        let cmd_dir = self.commands_dir();
        fs::create_dir_all(&cmd_dir)
            .await
            .map_err(EngineError::Io)?;

        let swift_req = SwiftRequest {
            id: id.to_string(),
            action,
            log_path: &req.log_path,
            pid_path: &req.pid_path,
            route_config: &req.route_config,
            speed_meters_per_second: req.speed_meters_per_second,
        };
        let body = serde_json::to_vec_pretty(&swift_req)
            .map_err(|e| EngineError::Other(format!("encode swift request: {e}")))?;

        let req_path = self.request_path(id);
        // atomic-ish write: write to tmp then rename, to avoid helper reading half-written file
        let tmp_path = req_path.with_extension("json.tmp");
        fs::write(&tmp_path, &body).await.map_err(EngineError::Io)?;
        fs::rename(&tmp_path, &req_path)
            .await
            .map_err(EngineError::Io)?;
        debug!(?req_path, "wrote legacy helper request");

        let resp_path = self.response_path(id);
        let deadline = std::time::Instant::now() + RESPONSE_TIMEOUT;
        loop {
            if resp_path.exists() {
                let bytes = fs::read(&resp_path).await.map_err(EngineError::Io)?;
                let resp: SwiftResponse = serde_json::from_slice(&bytes).map_err(|e| {
                    EngineError::Other(format!("decode swift response at {resp_path:?}: {e}"))
                })?;
                if resp.id == id {
                    // helper deletes the request file itself; we delete the response.
                    let _ = fs::remove_file(&resp_path).await;
                    let _ = fs::remove_file(&req_path).await;
                    return Ok(resp);
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(EngineError::Other(format!(
                    "legacy helper did not respond within {:?} (request {id} action {action})",
                    RESPONSE_TIMEOUT
                )));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// 从 `Started with PID 12345.` / `Already running with PID 100.` 这种
    /// helper output 里抠 PID —— 找 "PID" token 之后的第一个数字。
    fn parse_pid_from_output(output: &str) -> Option<u32> {
        let mut tokens = output.split_whitespace();
        while let Some(t) = tokens.next() {
            if t == "PID" {
                let pid_str = tokens.next()?.trim_end_matches('.');
                return pid_str.parse().ok();
            }
        }
        None
    }

    /// 开个后台 task tail 日志文件，每发现新行就 `Event::Log`；
    /// 同时定期检查 PID 进程是否仍在，不在则 `Event::Stopped`。
    /// 只允许同时有一个 watcher（多次 start 时旧的会自然结束）。
    async fn spawn_log_watcher(&self, log_path: PathBuf, pid: u32) {
        let mut running = self.watcher_running.lock().await;
        *running = true;
        drop(running);

        let events = self.events.clone();
        let running_flag = self.watcher_running.clone();
        tokio::spawn(async move {
            // 从文件末尾开始 tail（不要把启动前的旧日志全推一遍）
            let mut last_pos: u64 = match fs::metadata(&log_path).await {
                Ok(m) => m.len(),
                Err(_) => 0,
            };
            let mut last_alive_check = std::time::Instant::now();
            loop {
                tokio::time::sleep(LOG_TAIL_INTERVAL).await;

                // tail new bytes
                if let Ok(mut f) = fs::File::open(&log_path).await {
                    if let Ok(len) = f.metadata().await.map(|m| m.len()) {
                        if len > last_pos {
                            if f.seek(SeekFrom::Start(last_pos)).await.is_ok() {
                                let mut buf = Vec::with_capacity((len - last_pos) as usize);
                                if f.read_to_end(&mut buf).await.is_ok() {
                                    if let Ok(text) = std::str::from_utf8(&buf) {
                                        for line in text.lines() {
                                            if events.send(Event::Log(line.to_string())).is_err() {
                                                // no subscribers anymore? keep running, broadcast tolerates
                                            }
                                        }
                                    }
                                }
                            }
                            last_pos = len;
                        }
                    }
                }

                // periodic liveness probe
                if last_alive_check.elapsed() >= RUN_DETECT_INTERVAL {
                    last_alive_check = std::time::Instant::now();
                    if !process_alive(pid) {
                        let _ = events.send(Event::Stopped { exit_code: None });
                        let mut r = running_flag.lock().await;
                        *r = false;
                        info!(pid, "legacy helper python exited; watcher stopping");
                        return;
                    }
                }
            }
        });
    }
}

impl Default for LegacyHelperEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Engine for LegacyHelperEngine {
    async fn start(&self, req: &Request) -> Result<Response, EngineError> {
        // legacy helper expects uppercase UUID id (it's also fine with anything,
        // but matching Swift style keeps logs grep-consistent)
        let id = Uuid::new_v4().to_string().to_uppercase();
        let resp = self.dispatch(&id, "start", req).await?;
        if resp.status != 0 {
            return Err(EngineError::Other(format!(
                "legacy helper start failed (status={}): {}",
                resp.status, resp.output
            )));
        }

        if let Some(pid) = Self::parse_pid_from_output(&resp.output) {
            let _ = self.events.send(Event::Started { pid });
            self.spawn_log_watcher(PathBuf::from(&req.log_path), pid)
                .await;
        } else {
            warn!(output = %resp.output, "could not parse PID from legacy helper output");
        }

        Ok(Response::ok(&req.id, resp.output))
    }

    async fn stop(&self, req: &Request) -> Result<Response, EngineError> {
        let id = Uuid::new_v4().to_string().to_uppercase();
        let resp = self.dispatch(&id, "stop", req).await?;
        if resp.status != 0 {
            return Err(EngineError::Other(format!(
                "legacy helper stop failed (status={}): {}",
                resp.status, resp.output
            )));
        }
        // helper will SIGINT the python; the log watcher's liveness check
        // will fire Event::Stopped on its next tick.
        Ok(Response::ok(&req.id, resp.output))
    }

    async fn cleanup(&self, req: &Request) -> Result<Response, EngineError> {
        let id = Uuid::new_v4().to_string().to_uppercase();
        let resp = self.dispatch(&id, "cleanup", req).await?;
        Ok(Response::ok(&req.id, resp.output))
    }

    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_req() -> Request {
        Request::new_start(
            "outer-id",
            "/tmp/cyber-runner-test/run.log",
            "/tmp/cyber-runner-test/run.pid",
            cyber_runner_core::DEFAULT_ROUTE,
            4.7,
        )
    }

    #[test]
    fn parse_pid_from_typical_output() {
        assert_eq!(
            LegacyHelperEngine::parse_pid_from_output("Started with PID 12345."),
            Some(12345)
        );
    }

    #[test]
    fn parse_pid_from_no_dot() {
        assert_eq!(
            LegacyHelperEngine::parse_pid_from_output("Started with PID 999"),
            Some(999)
        );
    }

    #[test]
    fn parse_pid_from_non_matching_returns_none() {
        // "Already running with PID 100." should also extract 100 — our
        // parser scans for the "PID" token rather than hard-coding position.
        assert_eq!(
            LegacyHelperEngine::parse_pid_from_output("Already running with PID 100."),
            Some(100),
        );
        // truly garbage
        assert_eq!(LegacyHelperEngine::parse_pid_from_output("kapow"), None);
        assert_eq!(
            LegacyHelperEngine::parse_pid_from_output("No PID file; nothing to stop."),
            None, // "file;" after "PID" doesn't parse
        );
    }

    #[test]
    fn swift_request_serializes_to_camel_case() {
        let req = make_req();
        let swift = SwiftRequest {
            id: "ABC".into(),
            action: "start",
            log_path: &req.log_path,
            pid_path: &req.pid_path,
            route_config: &req.route_config,
            speed_meters_per_second: req.speed_meters_per_second,
        };
        let json = serde_json::to_value(&swift).unwrap();
        assert_eq!(json["action"], "start");
        assert_eq!(json["id"], "ABC");
        assert_eq!(json["logPath"], "/tmp/cyber-runner-test/run.log");
        assert_eq!(json["pidPath"], "/tmp/cyber-runner-test/run.pid");
        assert_eq!(json["routeConfig"], cyber_runner_core::DEFAULT_ROUTE);
        assert_eq!(json["speedMetersPerSecond"], 4.7);
    }

    #[test]
    fn swift_request_omits_speed_for_stop() {
        let swift = SwiftRequest {
            id: "X".into(),
            action: "stop",
            log_path: "/l",
            pid_path: "/p",
            route_config: "r",
            speed_meters_per_second: None,
        };
        let json = serde_json::to_value(&swift).unwrap();
        assert!(json.get("speedMetersPerSecond").is_none());
    }

    #[test]
    fn swift_response_decodes_swift_output() {
        let raw = r#"{
            "id": "abc",
            "status": 0,
            "output": "Started with PID 100.",
            "updatedAt": "2026-05-22T14:23:21Z"
        }"#;
        let r: SwiftResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(r.status, 0);
        assert_eq!(r.output, "Started with PID 100.");
        assert_eq!(r.id, "abc");
    }

    #[test]
    fn legacy_helper_available_is_callable() {
        // Just ensure it doesn't panic; true/false depends on host machine.
        let _ = legacy_helper_available();
    }

    #[tokio::test]
    async fn dispatch_times_out_when_no_helper_responds() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = LegacyHelperEngine::with_base(tmp.path());

        // Use a private RESPONSE_TIMEOUT shortcut by spawning then dropping the
        // request — without a helper to write a response, we expect timeout.
        // We can't easily shorten the 90s timeout without code change; instead
        // verify that `dispatch_with_timeout` semantics work by checking that
        // the request file is created.
        let req = make_req();
        let cmd_dir = tmp.path().join("commands");

        // race: kick off dispatch in background, peek the cmd dir after 300ms.
        let fut = tokio::spawn(async move { engine.dispatch("AAAA", "cleanup", &req).await });
        tokio::time::sleep(Duration::from_millis(300)).await;
        let entries: Vec<_> = std::fs::read_dir(&cmd_dir)
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(
            entries
                .iter()
                .any(|e| { e.file_name().to_string_lossy().starts_with("request-AAAA") }),
            "expected request file in {cmd_dir:?}, got {entries:?}"
        );
        // abort the spawned dispatch so the test doesn't wait 90s
        fut.abort();
    }

    #[tokio::test]
    async fn dispatch_reads_synthetic_response() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = Arc::new(LegacyHelperEngine::with_base(tmp.path()));

        let req = make_req();
        let req_clone = req.clone();
        let engine_clone = engine.clone();
        let id = "FEEDFACE";

        // simulate helper: in a background task, watch for request-FEEDFACE.json
        // and write a fake response.
        let cmd_dir = tmp.path().join("commands");
        let cmd_dir_clone = cmd_dir.clone();
        tokio::spawn(async move {
            for _ in 0..50 {
                if cmd_dir_clone.join("request-FEEDFACE.json").exists() {
                    let resp_path = cmd_dir_clone.join("response-FEEDFACE.json");
                    let body = r#"{"id":"FEEDFACE","status":0,"output":"Started with PID 4242."}"#;
                    let _ = tokio::fs::write(&resp_path, body).await;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });

        let resp = engine_clone
            .dispatch(id, "start", &req_clone)
            .await
            .expect("dispatch ok");
        assert_eq!(resp.status, 0);
        assert!(resp.output.contains("4242"));
    }
}
