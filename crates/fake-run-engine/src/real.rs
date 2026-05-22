//! Placeholder for [`RealEngine`], the production implementation of
//! [`crate::engine::Engine`]. Filled in by Phase 2 follow-up once
//! `cleanup` and `env_setup` are implemented.

use async_trait::async_trait;
use fake_run_core::{Request, Response};
use tokio::sync::mpsc;

use crate::engine::{Engine, EngineError, Event};

#[derive(Default)]
pub struct RealEngine;

impl RealEngine {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Engine for RealEngine {
    async fn start(&self, _req: &Request) -> Result<Response, EngineError> {
        Err(EngineError::Other(
            "RealEngine::start not yet implemented".into(),
        ))
    }

    async fn stop(&self, _req: &Request) -> Result<Response, EngineError> {
        Err(EngineError::Other(
            "RealEngine::stop not yet implemented".into(),
        ))
    }

    async fn cleanup(&self, _req: &Request) -> Result<Response, EngineError> {
        Err(EngineError::Other(
            "RealEngine::cleanup not yet implemented".into(),
        ))
    }

    fn subscribe(&self) -> mpsc::UnboundedReceiver<Event> {
        let (_tx, rx) = mpsc::unbounded_channel();
        rx
    }
}
