//! `fake-run-platform` — OS-specific privilege escalation and
//! service installation. Everything here is `cfg`-gated by target OS.

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_compiles() {}
}
