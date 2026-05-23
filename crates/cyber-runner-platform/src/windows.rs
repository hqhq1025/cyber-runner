//! Windows privilege checks.
//!
//! The Cyber Runner app is shipped with a UAC manifest requesting
//! `requireAdministrator`, so a normal user launch should already be elevated.
//! We expose [`is_running_elevated`] so the rest of the app can hard-fail
//! early (with a helpful error) if for some reason the elevation didn't
//! happen — e.g. when running `cargo run` from a non-admin developer shell
//! before the manifest is embedded into the binary.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

/// Returns `true` if the current process is running with an elevated
/// (Administrator) access token. Returns `false` on any Win32 error — the
/// caller should treat that the same as "not elevated".
pub fn is_running_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut ret_len: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        )
        .is_ok();

        let _ = CloseHandle(token);

        ok && elevation.TokenIsElevated != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: the function must at least be callable without panicking.
    /// We deliberately don't assert the return value because CI runners may
    /// or may not be elevated.
    #[test]
    fn elevated_check_is_callable() {
        // This module is `cfg(target_os = "windows")`-gated in `lib.rs`, so
        // reaching this test at runtime means we're on Windows.
        let _ = is_running_elevated();
    }
}
