//! `cyber-runner-engine` — orchestrates the Python engine subprocess,
//! streams logs, and exposes a high-level [`Engine`] trait that the
//! Tauri host invokes.

pub mod bundled_helper;
pub mod cleanup;
pub mod engine;
pub mod env_setup;
pub mod legacy_helper;
pub mod python_runner;
pub mod real;
pub mod runtime_install;

pub use bundled_helper::BundledHelperEngine;
pub use engine::{Engine, EngineError, Event};
pub use legacy_helper::{legacy_helper_available, LegacyHelperEngine};
pub use real::RealEngine;
pub use runtime_install::ensure_runtime_files;
