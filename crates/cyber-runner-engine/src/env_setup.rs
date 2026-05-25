//! venv / uv 检测与依赖安装，等价于 Swift `ensureDependencies()`。
//!
//! 优先用 `uv sync`（更快、有锁文件），否则降级到 `python -m venv` + `pip install -r requirements.txt`。

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;
use tokio::sync::broadcast::Sender;
use tracing::info;

use crate::engine::{EngineError, Event};

/// 候选 `uv` 可执行路径，第一个存在的胜出。
fn uv_candidates() -> Vec<PathBuf> {
    if cfg!(target_os = "windows") {
        let mut v = Vec::new();
        if let Some(home) = dirs::home_dir() {
            v.push(home.join(".local").join("bin").join("uv.exe"));
            v.push(home.join(".cargo").join("bin").join("uv.exe"));
        }
        v
    } else {
        vec![
            PathBuf::from("/opt/homebrew/bin/uv"),
            PathBuf::from("/usr/local/bin/uv"),
            PathBuf::from("/usr/bin/uv"),
        ]
    }
}

/// Locate uv on disk, returning the first existing candidate.
pub fn find_uv() -> Option<PathBuf> {
    uv_candidates().into_iter().find(|p| p.exists())
}

/// Ensure deps installed in `runtime_dir/.venv`. Emits each output line as
/// `Event::Log`. Returns Ok on success, Err with stderr-ish output otherwise.
pub async fn ensure_deps(runtime_dir: &Path, events: &Sender<Event>) -> Result<(), EngineError> {
    // Fast path: if .venv/bin/python already exists, assume deps are installed
    // and skip the uv/pip dance. Critical when runtime_dir points at an
    // already-prepared venv (Stage 4 bundled runtime, or — during dev — a
    // symlink to legacy RunControl/runtime which has pymobiledevice3 set up).
    // Without this guard, `uv sync` would run pointlessly each ▶ press,
    // wasting a few seconds and potentially failing if pyproject.toml is
    // missing.
    let venv_python = crate::python_runner::venv_python_path(runtime_dir);
    if venv_python.exists() {
        let _ = events.send(Event::Log(format!(
            "venv ready at {}, skipping ensure_deps",
            venv_python.display()
        )));
        info!(venv = %venv_python.display(), "venv exists, skipping ensure_deps");
        return Ok(());
    }

    if let Some(uv) = find_uv() {
        info!(uv = %uv.display(), "using uv sync");
        return run_streaming(
            &uv,
            &["sync"],
            runtime_dir,
            &[(
                "UV_CACHE_DIR",
                &format!("{}/.uv-cache", runtime_dir.display()),
            )],
            events,
        )
        .await;
    }

    info!("uv not found, falling back to pip");
    let venv_python = crate::python_runner::venv_python_path(runtime_dir);

    if !venv_python.exists() {
        let system_python = find_system_python().ok_or_else(|| {
            EngineError::PythonNotFound(
                "no python found (tried python.exe, py.exe, Windows Store, standard install paths)"
                    .into(),
            )
        })?;
        run_streaming(
            &system_python,
            &["-m", "venv", ".venv"],
            runtime_dir,
            &[],
            events,
        )
        .await?;
    }

    run_streaming(
        &venv_python,
        &["-m", "pip", "install", "--upgrade", "pip"],
        runtime_dir,
        &[],
        events,
    )
    .await?;
    run_streaming(
        &venv_python,
        &["-m", "pip", "install", "-r", "requirements.txt"],
        runtime_dir,
        &[],
        events,
    )
    .await
}

fn find_system_python() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(target_os = "windows") {
        let mut v = vec![
            PathBuf::from("python.exe"),
            PathBuf::from(r"C:\Python312\python.exe"),
            PathBuf::from(r"C:\Python311\python.exe"),
            PathBuf::from(r"C:\Python310\python.exe"),
            // py.exe launcher (installed with Python via official installer)
            PathBuf::from(r"C:\Windows\py.exe"),
        ];
        // Windows Store Python and standard per-user install
        if let Some(local) = dirs::data_local_dir() {
            v.push(local.join(r"Microsoft\WindowsApps\python.exe"));
            v.push(local.join(r"Programs\Python\Python312\python.exe"));
            v.push(local.join(r"Programs\Python\Python311\python.exe"));
            v.push(local.join(r"Programs\Python\Python310\python.exe"));
        }
        v
    } else {
        vec![
            PathBuf::from("/opt/homebrew/bin/python3"),
            PathBuf::from("/usr/local/bin/python3"),
            PathBuf::from("/usr/bin/python3"),
        ]
    };
    candidates.into_iter().find(|p| p.exists())
}

async fn run_streaming(
    exe: &Path,
    args: &[&str],
    cwd: &Path,
    env: &[(&str, &str)],
    events: &Sender<Event>,
) -> Result<(), EngineError> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut cmd = Command::new(exe);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }

    let mut child = cmd.spawn().map_err(EngineError::Spawn)?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    if let Some(stdout) = stdout {
        let tx = events.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = tx.send(Event::Log(line));
            }
        });
    }
    if let Some(stderr) = stderr {
        let tx = events.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = tx.send(Event::Log(line));
            }
        });
    }

    let status = child.wait().await?;
    if !status.success() {
        return Err(EngineError::DepsInstall(format!(
            "{} {:?} failed with status {:?}",
            exe.display(),
            args,
            status
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uv_candidates_are_platform_specific() {
        let cands = uv_candidates();
        assert!(!cands.is_empty());
        let first = cands[0].to_string_lossy();
        #[cfg(target_os = "windows")]
        assert!(first.ends_with("uv.exe"));
        #[cfg(not(target_os = "windows"))]
        assert!(first.ends_with("uv"));
    }

    #[test]
    fn find_uv_returns_existing_or_none() {
        // Just check the function doesn't panic and returns a real path if any.
        if let Some(p) = find_uv() {
            assert!(p.exists());
        }
    }
}
