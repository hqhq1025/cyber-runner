//! 跨平台 Python 子进程封装。
//!
//! - mac/linux: spawn `<runtime>/.venv/bin/python main.py`
//! - windows:   spawn `<runtime>\.venv\Scripts\python.exe main.py`
//!
//! stdout/stderr 合并、按行推到 [`mpsc::UnboundedSender<Event>`]。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc::UnboundedSender;
use tracing::warn;

use crate::engine::{EngineError, Event};

/// Resolve the platform-appropriate path to the venv's `python` binary.
pub fn venv_python_path(runtime_dir: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        runtime_dir.join(".venv").join("Scripts").join("python.exe")
    } else {
        runtime_dir.join(".venv").join("bin").join("python")
    }
}

/// Spawn `python <script>` with the given env. stdout/stderr are merged and
/// streamed as `Event::Log` lines on `events`. Returns the running [`Child`]
/// so the caller can later `kill` or `wait` on it.
pub async fn spawn_python(
    python_path: &Path,
    script: &str,
    runtime_dir: &Path,
    env: HashMap<String, String>,
    events: UnboundedSender<Event>,
) -> Result<Child, EngineError> {
    if !python_path.exists() {
        return Err(EngineError::PythonNotFound(
            python_path.display().to_string(),
        ));
    }

    let mut cmd = Command::new(python_path);
    cmd.arg(script)
        .current_dir(runtime_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);

    for (k, v) in &env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(EngineError::Spawn)?;

    // Stream stdout
    if let Some(stdout) = child.stdout.take() {
        let tx = events.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx.send(Event::Log(line)).is_err() {
                    break;
                }
            }
        });
    }

    // Stream stderr
    if let Some(stderr) = child.stderr.take() {
        let tx = events.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if tx.send(Event::Log(line)).is_err() {
                    break;
                }
            }
        });
    }

    Ok(child)
}

/// Terminate a process by PID gracefully (SIGINT/CTRL_C → SIGTERM/taskkill → SIGKILL),
/// returning `true` if the process is gone after the attempts.
pub async fn terminate_pid(pid: u32, grace_secs: u64) -> bool {
    // Try graceful first.
    let graceful_ok = try_signal_and_wait(pid, false, grace_secs).await;
    if graceful_ok {
        return true;
    }
    // Force.
    try_signal_and_wait(pid, true, 2).await
}

async fn try_signal_and_wait(pid: u32, force: bool, wait_secs: u64) -> bool {
    #[cfg(unix)]
    {
        let sig = if force { "KILL" } else { "INT" };
        let _ = std::process::Command::new("/bin/kill")
            .arg(format!("-{}", sig))
            .arg(pid.to_string())
            .status();
    }
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T"]);
        if force {
            cmd.arg("/F");
        }
        let _ = cmd.status();
    }

    for _ in 0..(wait_secs * 4) {
        if !process_alive(pid) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    if process_alive(pid) {
        warn!(pid, force, "process did not exit within wait window");
        false
    } else {
        true
    }
}

/// Cheap "is this PID still running" probe. Uses `sysinfo` to stay portable.
pub fn process_alive(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};
    let sys =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));
    sys.process(Pid::from_u32(pid)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn venv_python_path_mac() {
        let p = venv_python_path(Path::new("/tmp/runtime"));
        #[cfg(target_os = "windows")]
        assert!(p.to_string_lossy().ends_with(r".venv\Scripts\python.exe"));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(p, PathBuf::from("/tmp/runtime/.venv/bin/python"));
    }

    #[tokio::test]
    async fn spawn_missing_python_returns_not_found() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = spawn_python(
            Path::new("/does/not/exist/python"),
            "x.py",
            Path::new("/tmp"),
            HashMap::new(),
            tx,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, EngineError::PythonNotFound(_)));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn spawn_real_python_streams_lines() {
        let python = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("command -v python3")
            .output()
            .unwrap();
        let python_path = String::from_utf8(python.stdout).unwrap().trim().to_string();
        if python_path.is_empty() {
            eprintln!("skip: no python3 in PATH");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("emit.py");
        std::fs::write(
            &script,
            "import time\nfor i in range(3):\n    print(f'line-{i}', flush=True)\n    time.sleep(0.05)\n",
        )
        .unwrap();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut child = spawn_python(
            Path::new(&python_path),
            script.to_str().unwrap(),
            dir.path(),
            HashMap::new(),
            tx,
        )
        .await
        .unwrap();

        let status = child.wait().await.unwrap();
        assert!(status.success(), "python exited with {:?}", status);

        let mut lines = Vec::new();
        while let Ok(evt) = rx.try_recv() {
            if let Event::Log(s) = evt {
                lines.push(s);
            }
        }
        // Wait briefly for buffered output to drain.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        while let Ok(evt) = rx.try_recv() {
            if let Event::Log(s) = evt {
                lines.push(s);
            }
        }
        assert!(
            lines.iter().any(|l| l == "line-0"),
            "expected 'line-0', got {lines:?}"
        );
        assert!(lines.iter().any(|l| l == "line-2"));
    }
}
