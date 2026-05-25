//! `BundledHelperEngine` —— 把请求通过文件 IPC 发给 **我们自家的**
//! `cyber-runner-helper` 二进制（与 [`crate::legacy_helper::LegacyHelperEngine`]
//! 对比：legacy 寄生在已经装好的 Swift `RunControlHelper` 上）。
//!
//! 生命周期：
//! 1. `new(helper_path, base)` 构造，不做 IO
//! 2. 第一次 dispatch 时 `ensure_running()`：
//!    - 检测 `<base>/helper.pid` + 进程是否活
//!    - 不活就走 osascript 启动 helper as root（**这一步弹密码框**）
//!    - 等到 helper.pid 出现
//! 3. 之后每个 start/stop/cleanup 走 file IPC，无密码框
//! 4. `shutdown()` 发 `Action::Shutdown`，helper 干净退出，pid 文件清掉

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cyber_runner_core::{Request, Response};
use tokio::fs;
use tokio::sync::broadcast::{self, Sender};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::engine::{Engine, EngineError, Event};
use crate::python_runner::process_alive;

const EVENT_CAPACITY: usize = 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);
const HELPER_READY_TIMEOUT: Duration = Duration::from_secs(30);

pub struct BundledHelperEngine {
    helper_path: PathBuf,
    base: PathBuf,
    events: Sender<Event>,
    /// 一旦 ensure_running 成功一次就不再重复 spawn。Mutex 让多 task 并发安全。
    helper_ready: Arc<Mutex<bool>>,
}

impl BundledHelperEngine {
    pub fn new(helper_path: impl Into<PathBuf>, base: impl Into<PathBuf>) -> Self {
        let (tx, _rx) = broadcast::channel(EVENT_CAPACITY);
        Self {
            helper_path: helper_path.into(),
            base: base.into(),
            events: tx,
            helper_ready: Arc::new(Mutex::new(false)),
        }
    }

    fn commands_dir(&self) -> PathBuf {
        self.base.join("commands")
    }
    fn helper_pid_path(&self) -> PathBuf {
        self.base.join("helper.pid")
    }
    fn request_path(&self, id: &str) -> PathBuf {
        self.commands_dir().join(format!("request-{id}.json"))
    }
    fn response_path(&self, id: &str) -> PathBuf {
        self.commands_dir().join(format!("response-{id}.json"))
    }

    /// 检测 helper 是否在跑：读 helper.pid + process_alive。
    async fn helper_alive(&self) -> bool {
        let Ok(text) = fs::read_to_string(self.helper_pid_path()).await else {
            return false;
        };
        let Ok(pid) = text.trim().parse::<u32>() else {
            return false;
        };
        process_alive(pid)
    }

    /// 必要时启动 helper。**macOS 上会弹密码框**（osascript admin），Windows 上
    /// 依赖 release 的 UAC manifest 让整个 app 已是 admin，spawn 子进程继承权限。
    /// 已在跑就 noop。返回时保证 helper 已在 polling。
    pub async fn ensure_running(&self) -> Result<(), EngineError> {
        let mut ready = self.helper_ready.lock().await;
        if *ready && self.helper_alive().await {
            return Ok(());
        }

        // 如果 pid 文件还在但进程已死，清掉避免误判
        let _ = fs::remove_file(self.helper_pid_path()).await;

        info!(?self.helper_path, ?self.base, "spawning helper");
        self.spawn_helper_platform().await?;

        // 等 helper.pid 出现 & 进程活 —— 平台无关
        let deadline = std::time::Instant::now() + HELPER_READY_TIMEOUT;
        loop {
            if self.helper_alive().await {
                *ready = true;
                info!("helper is ready");
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(EngineError::Other(
                    "helper did not become ready within timeout".into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// macOS：通过 osascript 走 "do shell script ... with administrator privileges"
    /// 弹一次系统密码框，把 helper 以 root 启动 + 让它 detach 走 launchd 收养。
    ///
    /// Why 这个 dance: 不能用 `nohup`。`do shell script with administrator privileges`
    /// 给的不是真 TTY，macOS 的 nohup 在 tcgetattr 上失败
    /// (`Inappropriate ioctl for device`) 立刻退出，helper 根本起不来。
    /// 改用 `( cmd </dev/null >>log 2>&1 & )` 模式：
    ///   - 子 shell `(...)` 包起来 → 内层 shell 立刻退出，helper 被 launchd 收养
    ///   - `</dev/null` → osascript 的 stdin pipe 不再被持有，osascript 才能干净返回
    ///   - `>>log 2>&1` → stdout/stderr 落盘，不挂在 osascript 的 pipe 上
    ///   - 没有 controlling TTY → 没人会发 SIGHUP，nohup 多余
    #[cfg(target_os = "macos")]
    async fn spawn_helper_platform(&self) -> Result<(), EngineError> {
        let helper = self.helper_path.to_string_lossy().to_string();
        let base = self.base.to_string_lossy().to_string();
        let stdout_log = self.base.join("helper.stdout.log");
        let stdout_log = stdout_log.to_string_lossy().to_string();

        let shell_cmd = format!(
            "/bin/mkdir -p {base_q} && ( {helper_q} {base_q} </dev/null >>{log_q} 2>&1 & )",
            helper_q = sh_quote(&helper),
            base_q = sh_quote(&base),
            log_q = sh_quote(&stdout_log),
        );
        // AppleScript 双引号字符串内：\ 和 " 要转义
        let as_cmd = applescript_escape(&shell_cmd);
        let script = format!("do shell script \"{as_cmd}\" with administrator privileges");

        info!("invoking osascript (password prompt)");
        let output = tokio::process::Command::new("/usr/bin/osascript")
            .args(["-e", &script])
            .output()
            .await
            .map_err(EngineError::Spawn)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(EngineError::Other(format!(
                "osascript failed (status {}): {}",
                output.status,
                stderr.trim()
            )));
        }
        Ok(())
    }

    /// Windows：直接 spawn helper 子进程并 detach。
    ///
    /// 权限模型 —— 我们**不**在 spawn 时单独提升权限，而是依赖 release bundle 的
    /// UAC manifest（`requireAdministrator`）让整个 Cyber Runner app 一开始就以
    /// admin 启动；spawn 出来的子进程默认继承 admin token，于是 helper 也 admin。
    /// dev 模式（cargo run）跑出来的 app 是普通权限，spawn 出的 helper 也是普通权限
    /// —— 真机 USB / Mobile Device Service 访问会受限，但能完成"编译过 + 起空窗口"
    /// 的最小目标（与 GOAL.md 一致）。
    ///
    /// CREATE_NO_WINDOW (0x08000000)：不弹控制台黑框
    /// DETACHED_PROCESS (0x00000008)：脱离父进程 console，父进程退出不影响 helper
    #[cfg(target_os = "windows")]
    async fn spawn_helper_platform(&self) -> Result<(), EngineError> {
        // tokio::process::Command 在 Windows 上有 inherent `creation_flags`，
        // 不需要 import std::os::windows::process::CommandExt
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;

        fs::create_dir_all(&self.base)
            .await
            .map_err(EngineError::Io)?;

        let stdout_log = self.base.join("helper.stdout.log");
        let stdout_file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&stdout_log)
            .map_err(EngineError::Io)?;
        let stderr_file = stdout_file.try_clone().map_err(EngineError::Io)?;

        tokio::process::Command::new(&self.helper_path)
            .arg(&self.base)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout_file))
            .stderr(std::process::Stdio::from(stderr_file))
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .map_err(EngineError::Spawn)?;
        Ok(())
    }

    /// Linux / 其它 Unix fallback：直接 spawn。用户应该自己 sudo / pkexec 拉起整个
    /// app；这里不内置 pkexec/gksu 弹框（distro 太多不好统一）。如果需要 root 但没
    /// 拿到，helper 会在 pymobiledevice3 那一步报错。
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    async fn spawn_helper_platform(&self) -> Result<(), EngineError> {
        fs::create_dir_all(&self.base)
            .await
            .map_err(EngineError::Io)?;

        let stdout_log = self.base.join("helper.stdout.log");
        let stdout_file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&stdout_log)
            .map_err(EngineError::Io)?;
        let stderr_file = stdout_file.try_clone().map_err(EngineError::Io)?;

        tokio::process::Command::new(&self.helper_path)
            .arg(&self.base)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout_file))
            .stderr(std::process::Stdio::from(stderr_file))
            .spawn()
            .map_err(EngineError::Spawn)?;
        Ok(())
    }

    /// 发 `Action::Shutdown` 请求让 helper 干净退出（不需要 root）。
    /// 内部 helper —— 不要直接调，调 `<Self as Engine>::shutdown(&self)` 才对外
    pub async fn shutdown_helper(&self) -> Result<(), EngineError> {
        if !self.helper_alive().await {
            return Ok(());
        }
        let id = Uuid::new_v4().to_string();
        let req = Request::new_shutdown(&id);
        let resp = self.dispatch(&req).await?;
        debug!(?resp, "shutdown ack");
        // 等 pid 文件消失
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if !self.helper_pid_path().exists() {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        warn!("helper did not clean up pid file within 5s; assuming stuck");
        Ok(())
    }

    /// 写 request → 等 response → 解码。
    async fn dispatch(&self, req: &Request) -> Result<Response, EngineError> {
        let cmd_dir = self.commands_dir();
        fs::create_dir_all(&cmd_dir)
            .await
            .map_err(EngineError::Io)?;

        let body = serde_json::to_vec_pretty(&req)
            .map_err(|e| EngineError::Other(format!("encode request: {e}")))?;
        let req_path = self.request_path(&req.id);
        let tmp_path = req_path.with_extension("json.tmp");
        fs::write(&tmp_path, &body).await.map_err(EngineError::Io)?;
        fs::rename(&tmp_path, &req_path)
            .await
            .map_err(EngineError::Io)?;
        debug!(?req_path, action = ?req.action, "wrote request");

        let resp_path = self.response_path(&req.id);
        let deadline = std::time::Instant::now() + RESPONSE_TIMEOUT;
        loop {
            if resp_path.exists() {
                let bytes = fs::read(&resp_path).await.map_err(EngineError::Io)?;
                let resp: Response = serde_json::from_slice(&bytes).map_err(|e| {
                    EngineError::Other(format!("decode response at {resp_path:?}: {e}"))
                })?;
                if resp.id == req.id {
                    let _ = fs::remove_file(&resp_path).await;
                    return Ok(resp);
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(EngineError::Other(format!(
                    "helper did not respond within {:?} (request {})",
                    RESPONSE_TIMEOUT, req.id
                )));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

#[async_trait]
impl Engine for BundledHelperEngine {
    async fn start(&self, req: &Request) -> Result<Response, EngineError> {
        self.ensure_running().await?;
        let resp = self.dispatch(req).await?;
        if resp.status != 0 {
            return Err(EngineError::Other(format!(
                "helper start failed (status={}): {}",
                resp.status, resp.output
            )));
        }
        // 解析 PID（"Started with PID 12345."）触发 Event::Started
        if let Some(pid) = parse_pid(&resp.output) {
            let _ = self.events.send(Event::Started { pid });
        }
        Ok(resp)
    }

    async fn stop(&self, req: &Request) -> Result<Response, EngineError> {
        self.ensure_running().await?;
        let resp = self.dispatch(req).await?;
        if resp.status != 0 {
            return Err(EngineError::Other(format!(
                "helper stop failed (status={}): {}",
                resp.status, resp.output
            )));
        }
        let _ = self.events.send(Event::Stopped { exit_code: None });
        Ok(resp)
    }

    async fn cleanup(&self, req: &Request) -> Result<Response, EngineError> {
        self.ensure_running().await?;
        let resp = self.dispatch(req).await?;
        Ok(resp)
    }

    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// 优雅停 helper：发 Action::Shutdown，helper 写 response 后 break loop。
    async fn shutdown(&self) -> Result<(), EngineError> {
        self.shutdown_helper().await
    }
}

fn parse_pid(output: &str) -> Option<u32> {
    let mut toks = output.split_whitespace();
    while let Some(t) = toks.next() {
        if t == "PID" {
            return toks.next()?.trim_end_matches('.').parse().ok();
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn sh_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(target_os = "macos")]
fn applescript_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pid_typical() {
        assert_eq!(parse_pid("Started with PID 12345."), Some(12345));
        assert_eq!(parse_pid("Already running with PID 999"), Some(999));
        assert_eq!(parse_pid("nothing here"), None);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn sh_quote_basic() {
        assert_eq!(sh_quote("hello"), "'hello'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        assert_eq!(sh_quote("a b"), "'a b'");
        assert_eq!(sh_quote("$HOME"), "'$HOME'");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn applescript_escape_basic() {
        assert_eq!(applescript_escape(r"\"), r"\\");
        assert_eq!(applescript_escape("\""), "\\\"");
        assert_eq!(applescript_escape("plain"), "plain");
    }

    #[tokio::test]
    async fn helper_alive_returns_false_with_no_pid_file() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = BundledHelperEngine::new("/no/helper", tmp.path());
        assert!(!engine.helper_alive().await);
    }

    #[tokio::test]
    async fn shutdown_is_noop_when_helper_not_running() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = BundledHelperEngine::new("/no/helper", tmp.path());
        assert!(engine.shutdown_helper().await.is_ok());
    }
}
