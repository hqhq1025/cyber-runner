//! `cyber-runner-platform` — OS-specific privilege escalation and
//! service installation. Everything here is `cfg`-gated by target OS.

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_compiles() {}
}
