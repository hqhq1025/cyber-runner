//! Engine trait + shared event/error types.

use async_trait::async_trait;
use fake_run_core::{Request, Response};
use tokio::sync::mpsc;

/// Events streamed from the engine to subscribers (Tauri host → frontend).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// One line of stdout/stderr captured from the Python subprocess.
    Log(String),
    /// The Python subprocess started successfully.
    Started { pid: u32 },
    /// The Python subprocess exited; `exit_code` is `None` if killed.
    Stopped { exit_code: Option<i32> },
    /// Engine raised an error.
    Error(String),
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("python binary not found at {0}")]
    PythonNotFound(String),
    #[error("failed to spawn python: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("already running with PID {pid}")]
    AlreadyRunning { pid: u32 },
    #[error("dependency install failed: {0}")]
    DepsInstall(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

#[async_trait]
pub trait Engine: Send + Sync {
    /// Start the engine (spawn Python main.py) for the given request.
    async fn start(&self, req: &Request) -> Result<Response, EngineError>;

    /// Stop the running engine.
    async fn stop(&self, req: &Request) -> Result<Response, EngineError>;

    /// Kill any orphan `pymobiledevice3` tunnel processes.
    async fn cleanup(&self, req: &Request) -> Result<Response, EngineError>;

    /// Subscribe to engine events. Each call returns a fresh receiver;
    /// implementations may multiplex internally.
    fn subscribe(&self) -> mpsc::UnboundedReceiver<Event>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_clones_and_compares() {
        let e = Event::Started { pid: 42 };
        assert_eq!(e.clone(), Event::Started { pid: 42 });
        assert_ne!(e, Event::Started { pid: 43 });
    }

    #[test]
    fn engine_error_already_running_displays_pid() {
        let err = EngineError::AlreadyRunning { pid: 1234 };
        assert!(err.to_string().contains("1234"));
    }

    #[test]
    fn engine_error_python_not_found_displays_path() {
        let err = EngineError::PythonNotFound("/nope/python".into());
        assert!(err.to_string().contains("/nope/python"));
    }
}
