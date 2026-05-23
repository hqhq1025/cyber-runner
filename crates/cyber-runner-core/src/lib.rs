//! `cyber-runner-core` — shared types and pure logic.
//!
//! No I/O, no platform-specific code. Everything here is unit-testable
//! without spawning processes or touching the filesystem.

pub mod coord;
pub mod path_smoothing;
pub mod paths;
pub mod protocol;
pub mod route;
pub mod run_state;
pub mod time_formatter;

pub use paths::Paths;
pub use protocol::{
    clamp_speed, Action, Request, Response, DEFAULT_ROUTE, MAX_SPEED_MPS, MIN_SPEED_MPS,
};
pub use route::{parse_route, Point, RouteError};
pub use run_state::running_pid_from;
pub use time_formatter::{duration_text, pace_text, remaining_seconds};
