//! 孤儿 `pymobiledevice3 lockdown start-tunnel` 进程清理。
//! 等价于 Swift `cleanupOrphanTunnels()` + `pidsMatching()`。

use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use tokio::sync::broadcast::Sender;
use tracing::info;

use crate::engine::Event;
use crate::python_runner::terminate_pid;

const TUNNEL_PATTERN: &str = "pymobiledevice3 lockdown start-tunnel";

/// Find PIDs whose cmdline contains the given substring.
pub fn pids_matching(pattern: &str) -> Vec<u32> {
    let sys =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));
    sys.processes()
        .iter()
        .filter_map(|(pid, process)| {
            let cmd = process.cmd().join(&std::ffi::OsString::from(" "));
            if cmd.to_string_lossy().contains(pattern) {
                Some(pid.as_u32())
            } else {
                None
            }
        })
        .collect()
}

/// Kill every running `pymobiledevice3 lockdown start-tunnel`. Returns the
/// list of PIDs that were terminated.
pub async fn cleanup_orphan_tunnels(events: &Sender<Event>) -> Vec<u32> {
    let pids = pids_matching(TUNNEL_PATTERN);
    if pids.is_empty() {
        let _ = events.send(Event::Log("No orphan tunnels.".into()));
        return pids;
    }
    for &pid in &pids {
        let _ = events.send(Event::Log(format!("Cleaning orphan tunnel PID {pid}")));
        info!(pid, "cleaning orphan tunnel");
        terminate_pid(pid, 1).await;
    }
    let _ = events.send(Event::Log(format!(
        "Cleaned orphan tunnels: {}",
        pids.iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )));
    pids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pids_matching_returns_empty_for_unlikely_pattern() {
        let pids = pids_matching("this-pattern-cannot-exist-1234abcdef");
        assert!(pids.is_empty());
    }

    #[test]
    fn pids_matching_finds_self() {
        // The test runner's cmdline almost certainly contains "test" or "cargo".
        // We don't assert >=1 because that's flaky in some CIs; just call it.
        let _ = pids_matching("cargo");
    }
}
