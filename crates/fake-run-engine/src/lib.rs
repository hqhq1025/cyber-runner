//! `fake-run-engine` — orchestrates the Python engine subprocess,
//! streams logs, and exposes a high-level [`Engine`] trait that the
//! Tauri host invokes.

pub mod cleanup;
pub mod engine;
pub mod env_setup;
pub mod python_runner;
pub mod real;

pub use engine::{Engine, EngineError, Event};
pub use real::RealEngine;
