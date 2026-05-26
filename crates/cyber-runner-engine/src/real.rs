//! [`RealEngine`] — production [`Engine`] implementation.
//!
//! Composes [`crate::python_runner`], [`crate::env_setup`] and
//! [`crate::cleanup`] to mirror Swift `handleStart` / `handleStop` /
//! `handleRequest` behaviour on `~/Library/Application Support/RunControl`,
//! but cross-platform and without a separate privileged helper process.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Local;
use cyber_runner_core::{Request, Response};
use tokio::sync::broadcast::{self, Sender};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::cleanup::cleanup_orphan_tunnels;
use crate::engine::{Engine, EngineError, Event};
use crate::env_setup::ensure_deps;
use crate::python_runner::{process_alive, spawn_python, terminate_pid, venv_python_path};

const EVENT_CAPACITY: usize = 1024;

/// Production engine. Holds a broadcast channel so the Tauri host can
/// subscribe and re-emit events to the frontend.
pub struct RealEngine {
    runtime_dir: PathBuf,
    events: Sender<Event>,
    running: Arc<Mutex<Option<u32>>>,
}

impl RealEngine {
    pub fn new(runtime_dir: impl Into<PathBuf>) -> Self {
        let (tx, _rx) = broadcast::channel(EVENT_CAPACITY);
        Self {
            runtime_dir: runtime_dir.into(),
            events: tx,
            running: Arc::new(Mutex::new(None)),
        }
    }

    /// Append a header line to the log file and broadcast it too.
    async fn write_log_header(&self, log_path: &str, header: &str) {
        let stamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let line = format!("===== {stamp} {header} =====\n");
        if let Some(parent) = std::path::Path::new(log_path).parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let write_result = tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(log_path)
            .await;
        match write_result {
            Ok(mut f) => {
                if let Err(err) = tokio::io::AsyncWriteExt::write_all(&mut f, line.as_bytes()).await
                {
                    warn!(?err, log_path, "log header write_all failed");
                }
            }
            Err(err) => warn!(?err, log_path, "could not open log for header"),
        }
        let _ = self.events.send(Event::Log(line.trim_end().to_string()));
    }

    async fn read_existing_pid(pid_path: &str) -> Option<u32> {
        let text = tokio::fs::read_to_string(pid_path).await.ok()?;
        cyber_runner_core::running_pid_from(&text).map(|p| p as u32)
    }

    async fn write_pid(pid_path: &str, pid: u32) -> Result<(), EngineError> {
        if let Some(parent) = std::path::Path::new(pid_path).parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        tokio::fs::write(pid_path, format!("{pid}\n"))
            .await
            .map_err(EngineError::Io)
    }

    async fn remove_pid(pid_path: &str) {
        let _ = tokio::fs::remove_file(pid_path).await;
    }

    fn build_env(req: &Request) -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("RUN_ROUTE_CONFIG".into(), req.route_config.clone());
        if let Some(speed) = req.speed_meters_per_second {
            env.insert("RUN_SPEED".into(), format!("{:.1}", speed));
        }
        env.insert("PYTHONUNBUFFERED".into(), "1".into());
        env
    }
}

#[async_trait]
impl Engine for RealEngine {
    async fn start(&self, req: &Request) -> Result<Response, EngineError> {
        let _ = cleanup_orphan_tunnels(&self.events).await;

        if let Some(pid) = Self::read_existing_pid(&req.pid_path).await {
            if process_alive(pid) {
                let _ = self.events.send(Event::Log(format!(
                    "Already running with PID {pid}; refusing to spawn"
                )));
                return Err(EngineError::AlreadyRunning { pid });
            }
        }

        // Archive previous log.
        if std::path::Path::new(&req.log_path).exists() {
            let archive = format!("{}.last", req.log_path);
            let _ = tokio::fs::remove_file(&archive).await;
            let _ = tokio::fs::rename(&req.log_path, &archive).await;
        }

        self.write_log_header(&req.log_path, "start requested")
            .await;
        ensure_deps(&self.runtime_dir, &self.events).await?;
        self.write_log_header(&req.log_path, "start").await;

        let python = venv_python_path(&self.runtime_dir);
        let env = Self::build_env(req);
        let mut child = spawn_python(
            &python,
            "main.py",
            &self.runtime_dir,
            env,
            self.events.clone(),
        )
        .await?;
        let pid = child
            .id()
            .ok_or_else(|| EngineError::Other("spawned child has no pid".into()))?;

        // Brief liveness check.
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        if !process_alive(pid) {
            return Err(EngineError::Other(format!(
                "python exited immediately (check {})",
                req.log_path
            )));
        }

        Self::write_pid(&req.pid_path, pid).await?;
        *self.running.lock().await = Some(pid);
        let _ = self.events.send(Event::Started { pid });
        info!(pid, "engine started");

        // Detach: wait on child in a background task so events fire on exit.
        let running = self.running.clone();
        let events = self.events.clone();
        tokio::spawn(async move {
            let status = child.wait().await.ok();
            let code = status.and_then(|s| s.code());
            let _ = events.send(Event::Stopped { exit_code: code });
            *running.lock().await = None;
        });

        Ok(Response::ok(&req.id, format!("Started with PID {pid}.")))
    }

    async fn stop(&self, req: &Request) -> Result<Response, EngineError> {
        let pid = match Self::read_existing_pid(&req.pid_path).await {
            Some(p) => p,
            None => return Ok(Response::ok(&req.id, "No PID file; nothing to stop.")),
        };
        if !process_alive(pid) {
            Self::remove_pid(&req.pid_path).await;
            return Ok(Response::ok(&req.id, "Saved PID is not running."));
        }
        self.write_log_header(&req.log_path, "stop").await;
        let ok = terminate_pid(pid, 5).await;
        let _ = cleanup_orphan_tunnels(&self.events).await;
        Self::remove_pid(&req.pid_path).await;
        *self.running.lock().await = None;
        let msg = if ok {
            "Stopped."
        } else {
            "Stopped with force cleanup."
        };
        Ok(Response::ok(&req.id, msg))
    }

    async fn cleanup(&self, req: &Request) -> Result<Response, EngineError> {
        let pids = cleanup_orphan_tunnels(&self.events).await;
        let msg = if pids.is_empty() {
            "No orphan tunnels.".to_string()
        } else {
            format!(
                "Cleaned orphan tunnels: {}",
                pids.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        Ok(Response::ok(&req.id, msg))
    }

    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cyber_runner_core::Request;

    #[tokio::test]
    async fn stop_with_no_pid_file_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("run.pid");
        let log_path = dir.path().join("run.log");
        let engine = RealEngine::new("/tmp");
        let req = Request::new_stop(
            "t1",
            log_path.to_string_lossy().to_string(),
            pid_path.to_string_lossy().to_string(),
        );
        let resp = engine.stop(&req).await.unwrap();
        assert_eq!(resp.status, 0);
        assert!(resp.output.contains("No PID file"));
    }

    #[tokio::test]
    async fn cleanup_returns_ok_when_no_orphans() {
        let engine = RealEngine::new("/tmp");
        let req = Request::new_cleanup("c1", "/tmp/run.log", "/tmp/run.pid");
        let resp = engine.cleanup(&req).await.unwrap();
        assert_eq!(resp.status, 0);
    }

    #[tokio::test]
    async fn subscribe_returns_independent_receivers() {
        let engine = RealEngine::new("/tmp");
        let mut a = engine.subscribe();
        let mut b = engine.subscribe();
        engine.events.send(Event::Log("hello".into())).unwrap();
        assert_eq!(a.recv().await.unwrap(), Event::Log("hello".into()));
        assert_eq!(b.recv().await.unwrap(), Event::Log("hello".into()));
    }

    #[tokio::test]
    async fn stop_removes_stale_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("run.pid");
        let log_path = dir.path().join("run.log");
        // Write a PID that almost certainly doesn't exist.
        tokio::fs::write(&pid_path, "999999\n").await.unwrap();
        let engine = RealEngine::new("/tmp");
        let req = Request::new_stop(
            "t-stale",
            log_path.to_string_lossy().to_string(),
            pid_path.to_string_lossy().to_string(),
        );
        let resp = engine.stop(&req).await.unwrap();
        assert!(resp.output.contains("Saved PID is not running"));
        assert!(!pid_path.exists());
    }

    /// This is the regression test for the bug user hit on 2026-05-26:
    /// closing the app left main.py orphaned (PPID=1) because helper's
    /// shutdown path only ran cleanup (orphan tunnels) and never invoked
    /// stop on the real run.pid. Helper main.rs now calls stop with the
    /// real paths before cleanup; this test asserts that path actually
    /// kills the live child instead of just returning OK.
    #[tokio::test]
    async fn stop_kills_alive_child_referenced_by_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        let pid_path = dir.path().join("run.pid");
        let log_path = dir.path().join("run.log");

        // Spawn a live child (sleep 60s) and point run.pid at it.
        // sleep is available on every unix CI runner; for Windows CI we
        // skip — Windows path uses taskkill, covered by python_runner tests.
        #[cfg(unix)]
        {
            use std::time::Duration;
            let mut child = tokio::process::Command::new("sleep")
                .arg("60")
                .spawn()
                .expect("spawn sleep");
            let pid = child.id().expect("get pid");
            tokio::fs::write(&pid_path, pid.to_string()).await.unwrap();

            assert!(
                crate::python_runner::process_alive(pid),
                "precondition: child {pid} alive"
            );

            let engine = RealEngine::new("/tmp");
            let req = Request::new_stop(
                "t-kill-alive",
                log_path.to_string_lossy().to_string(),
                pid_path.to_string_lossy().to_string(),
            );
            let resp = engine.stop(&req).await.unwrap();
            assert_eq!(resp.status, 0, "stop should succeed: {resp:?}");

            // Give the OS a moment to reap.
            tokio::time::sleep(Duration::from_millis(300)).await;
            assert!(
                !crate::python_runner::process_alive(pid),
                "child {pid} must be dead after stop()"
            );
            assert!(!pid_path.exists(), "run.pid must be removed");

            let _ = child.wait().await;
        }
        #[cfg(not(unix))]
        {
            let _ = (pid_path, log_path);
        }
    }
}
