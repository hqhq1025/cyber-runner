//! Placeholder for the orphan tunnel cleanup module. Real implementation
//! lives behind `pidsMatching("pymobiledevice3 lockdown start-tunnel")` in
//! Swift; the Rust port uses `sysinfo` to walk the process table and SIGTERM
//! anything matching the pattern. Filled in by Phase 2 follow-up.

#![allow(dead_code)]
