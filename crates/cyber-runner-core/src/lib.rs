//! Pure logic for Cyber Runner: datums, route parsing, track geometry and
//! time-based motion. No I/O, no device code — everything here is unit-tested.

pub mod format;
pub mod geo;
pub mod motion;
pub mod route;
pub mod track;

pub use geo::{Datum, LatLng};
pub use motion::{clamp_speed, Fix, Motion, Variation, MAX_SPEED_MPS, MIN_SPEED_MPS};
pub use route::{
    normalize, parse_points, polyline_length_m, BuiltinRoute, LoopMode, RouteError, BUILTIN_ROUTES,
};
pub use track::Track;
