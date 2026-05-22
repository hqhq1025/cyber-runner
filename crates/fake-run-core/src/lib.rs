//! `fake-run-core` — shared types and pure logic.
//!
//! No I/O, no platform-specific code. Everything here is unit-testable
//! without spawning processes or touching the filesystem.

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_compiles() {}
}
