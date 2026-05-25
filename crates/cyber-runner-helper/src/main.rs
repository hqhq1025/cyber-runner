//! `cyber-runner-helper` —— 根权限 background binary，由 Cyber Runner App 在
//! 启动时通过 `osascript ... with administrator privileges` 拉起。
//!
//! 职责：
//! 1. 监听 `<base>/commands/` 目录的 `request-*.json` 文件
//! 2. 解析为 [`cyber_runner_core::Request`]，分派给 [`cyber_runner_engine::RealEngine`]
//! 3. 把结果写回 `<base>/commands/response-<id>.json`
//! 4. 收到 SIGTERM / SIGINT 时干净退出（杀掉自己 spawn 的 python 子进程）
//!
//! 用法：
//! ```text
//! cyber-runner-helper <BASE_DIR>
//! # 例：
//! cyber-runner-helper "/Users/me/Library/Application Support/CyberRunner"
//! ```
//!
//! BASE_DIR 下的目录结构：
//! ```text
//! BASE_DIR/
//!   commands/                     ← 我们轮询这里
//!     request-<UUID>.json
//!     response-<UUID>.json
//!   runtime/                      ← Python venv + main.py 在这里
//!     .venv/bin/python
//!     main.py
//!   helper.log                    ← 我们写自己的运行日志
//!   run.log                       ← Python 引擎日志
//!   run.pid                       ← Python 引擎 PID
//! ```

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use chrono::Local;
use cyber_runner_core::{Request, Response};
use cyber_runner_engine::{Engine, RealEngine};
use tokio::fs;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let base = match parse_args() {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("cyber-runner-helper: {msg}");
            eprintln!("usage: cyber-runner-helper <BASE_DIR>");
            return ExitCode::from(2);
        }
    };

    let commands_dir = base.join("commands");
    let runtime_dir = base.join("runtime");
    let helper_log = base.join("helper.log");
    let helper_pid = base.join("helper.pid");

    if let Err(err) = fs::create_dir_all(&commands_dir).await {
        eprintln!("cyber-runner-helper: cannot create {commands_dir:?}: {err}");
        return ExitCode::from(1);
    }

    // Single-instance guard：如果 helper.pid 已存在且指向一个仍在运行的进程，
    // 说明已有 helper 在跑（典型场景：上一轮 tauri dev 退出但 root helper 因
    // 权限没被 SIGTERM 干掉，又被新 tauri 重新 osascript 拉起一份）。
    // 多 helper 共享同一个 commands/ 会导致同一 request 被并发处理 → 多 spawn
    // python，stop 只能干掉一个。直接退出让用户/启动器明确感知问题。
    if let Ok(existing) = fs::read_to_string(&helper_pid).await {
        if let Ok(existing_pid) = existing.trim().parse::<u32>() {
            if existing_pid != std::process::id()
                && cyber_runner_engine::python_runner::process_alive(existing_pid)
            {
                let msg = format!(
                    "another cyber-runner-helper (pid={existing_pid}) is already running; exiting"
                );
                eprintln!("cyber-runner-helper: {msg}");
                append_helper_log(&helper_log, &msg).await;
                return ExitCode::from(0);
            }
        }
    }

    // 把自己的 PID 写到 helper.pid，让 Tauri host 能检测活性 + 必要时直接 kill。
    if let Err(err) = fs::write(&helper_pid, std::process::id().to_string()).await {
        warn!(?err, ?helper_pid, "could not write helper.pid");
    }
    append_helper_log(
        &helper_log,
        &format!("helper started base={base:?} pid={}", std::process::id()),
    )
    .await;
    info!(
        ?base,
        pid = std::process::id(),
        "cyber-runner-helper started"
    );

    let engine = RealEngine::new(runtime_dir);

    // 把 SIGINT/SIGTERM 折叠成 mpsc<()>，主循环 select 处理
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<&'static str>(2);
    spawn_signal_listener(shutdown_tx.clone());

    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let exit_code = loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(err) = scan_once(&commands_dir, &engine, &helper_log, &shutdown_tx).await {
                    warn!(?err, "scan iteration failed");
                }
            }
            Some(sig) = shutdown_rx.recv() => {
                append_helper_log(&helper_log, &format!("received {sig}, shutting down")).await;
                info!(signal = sig, "shutting down");
                break ExitCode::SUCCESS;
            }
        }
    };

    // 退出前发一次 cleanup，杀掉孤儿 tunnel 进程
    let req = Request::new_cleanup(
        "shutdown",
        "/tmp/cyber-runner-helper-shutdown.log",
        "/tmp/cyber-runner-helper-shutdown.pid",
    );
    if let Err(err) = engine.cleanup(&req).await {
        warn!(?err, "shutdown cleanup failed");
    }
    // 移走 pid 文件
    let _ = fs::remove_file(&helper_pid).await;
    append_helper_log(&helper_log, "helper exited cleanly").await;
    exit_code
}

fn parse_args() -> Result<PathBuf, String> {
    let mut args = env::args().skip(1);
    let base = args
        .next()
        .ok_or_else(|| "missing BASE_DIR argument".to_string())?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    Ok(PathBuf::from(base))
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}

#[cfg(unix)]
fn spawn_signal_listener(tx: mpsc::Sender<&'static str>) {
    use tokio::signal::unix::{signal, SignalKind};
    tokio::spawn(async move {
        let term = signal(SignalKind::terminate());
        let int = signal(SignalKind::interrupt());
        match (term, int) {
            (Ok(mut term), Ok(mut int)) => {
                tokio::select! {
                    _ = term.recv() => { let _ = tx.send("SIGTERM").await; }
                    _ = int.recv() => { let _ = tx.send("SIGINT").await; }
                }
            }
            _ => {
                error!("failed to install signal handlers; helper won't shut down gracefully");
            }
        }
    });
}

/// Windows 没有 SIGTERM/SIGINT 语义；唯一的可移植入口是 Ctrl-C（CTRL_C_EVENT 或
/// console Close）。日常使用中 helper 退出主要靠两种方式：
///   1. Tauri host 在 WindowEvent::CloseRequested 时发 Action::Shutdown 请求 → 主循环 break
///   2. UAC-elevated app 关闭时，OS 把 console 拆掉触发 ctrl_c
/// 这里只挂 ctrl_c，剩下的优雅退路由 shutdown request 走。
#[cfg(windows)]
fn spawn_signal_listener(tx: mpsc::Sender<&'static str>) {
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            let _ = tx.send("CTRL_C").await;
        } else {
            error!("failed to install ctrl_c handler; rely on Action::Shutdown to exit");
        }
    });
}

async fn scan_once(
    commands_dir: &Path,
    engine: &RealEngine,
    helper_log: &Path,
    shutdown_tx: &mpsc::Sender<&'static str>,
) -> std::io::Result<()> {
    let mut entries = fs::read_dir(commands_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("request-") || !name.ends_with(".json") {
            continue;
        }
        handle_request(&path, engine, helper_log, shutdown_tx).await;
    }
    Ok(())
}

async fn handle_request(
    request_path: &Path,
    engine: &RealEngine,
    helper_log: &Path,
    shutdown_tx: &mpsc::Sender<&'static str>,
) {
    // 原子认领：在并发多 helper 场景下（单实例 guard 兜不住的极端情况，比如
    // 旧 helper 刚 crash 还没来得及清 helper.pid，新 helper 拉起来），多个进程
    // 可能同时在 scan_once 里看到同一个 request 文件。先 rename 成 .claimed —
    // POSIX rename 是 atomic 的，只有一个 helper 能成功；剩下的会拿到 ENOENT
    // 并安全跳过，绝不会出现"同一 request 被 spawn 多次 python"的灾难。
    let claimed_path = {
        let mut p = request_path.to_path_buf();
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("request-unknown.json")
            .to_string();
        p.set_file_name(format!("{name}.claimed"));
        p
    };
    if let Err(err) = fs::rename(request_path, &claimed_path).await {
        if err.kind() == std::io::ErrorKind::NotFound {
            // 另一个 helper / 这一轮自己上一 tick 已经处理过 → 无声跳过
            return;
        }
        warn!(?err, ?request_path, "could not claim request");
        return;
    }

    let body = match fs::read(&claimed_path).await {
        Ok(b) => b,
        Err(err) => {
            warn!(?err, ?claimed_path, "cannot read claimed request file");
            let _ = fs::remove_file(&claimed_path).await;
            return;
        }
    };
    let req: Request = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(err) => {
            warn!(?err, ?claimed_path, "cannot decode request");
            let _ = fs::remove_file(&claimed_path).await;
            return;
        }
    };

    append_helper_log(
        helper_log,
        &format!("request id={} action={:?}", req.id, req.action),
    )
    .await;

    let response = match req.action {
        cyber_runner_core::Action::Start => engine.start(&req).await,
        cyber_runner_core::Action::Stop => engine.stop(&req).await,
        cyber_runner_core::Action::Cleanup => engine.cleanup(&req).await,
        cyber_runner_core::Action::Shutdown => {
            // 写完响应再 break 主循环
            Ok(Response::ok(&req.id, "shutdown acknowledged"))
        }
    };

    let resp = match response {
        Ok(resp) => resp,
        Err(err) => Response::fail(&req.id, 1, err.to_string()),
    };

    let is_shutdown = matches!(req.action, cyber_runner_core::Action::Shutdown);

    let resp_path = request_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("response-{}.json", req.id));
    let encoded = match serde_json::to_vec_pretty(&resp) {
        Ok(b) => b,
        Err(err) => {
            warn!(?err, "cannot encode response");
            let _ = fs::remove_file(&claimed_path).await;
            return;
        }
    };
    if let Err(err) = fs::write(&resp_path, &encoded).await {
        warn!(?err, ?resp_path, "cannot write response");
    }
    // 删除 .claimed 标记文件（取代旧版的删除 request 文件）
    let _ = fs::remove_file(&claimed_path).await;
    append_helper_log(
        helper_log,
        &format!(
            "request id={} {}",
            req.id,
            if resp.status == 0 { "ok" } else { "fail" }
        ),
    )
    .await;

    if is_shutdown {
        let _ = shutdown_tx.send("Shutdown request").await;
    }
}

async fn append_helper_log(path: &Path, message: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }
    let stamp = Local::now().format("%Y-%m-%d %H:%M:%S %z");
    let line = format!("[{stamp}] {message}\n");
    let open = tokio::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .await;
    match open {
        Ok(mut f) => {
            if let Err(err) = tokio::io::AsyncWriteExt::write_all(&mut f, line.as_bytes()).await {
                eprintln!("cyber-runner-helper: helper.log write failed: {err}");
            }
        }
        Err(err) => eprintln!("cyber-runner-helper: cannot open helper.log: {err}"),
    }
}
