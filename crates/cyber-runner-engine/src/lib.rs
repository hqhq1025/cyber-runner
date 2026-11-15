//! Device side of Cyber Runner, in pure Rust on top of `idevice`.
//!
//! Pipeline: usbmuxd → lockdown (pair / trust) → AMFI developer-mode check →
//! personalized DDI mount → CoreDeviceProxy + in-process TCP stack → RSD →
//! DVT `LocationSimulation`. Runs unprivileged on macOS, Windows and Linux.

pub mod ddi;
pub mod device;
pub mod error;
pub mod link;
pub mod runner;

pub use device::{list_devices, DeviceInfo};
pub use error::{EngineError, ErrorInfo};
pub use runner::{
    DeviceList, Engine, EngineConfig, FinishAction, Level, LogLine, Phase, RunPlan, Snapshot,
    StopAfter,
};
