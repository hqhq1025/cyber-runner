//! `fake-run-engine` — orchestrates the Python engine subprocess,
//! streams logs, and exposes a high-level `Engine` trait that the
//! Tauri host invokes.

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_compiles() {}
}
