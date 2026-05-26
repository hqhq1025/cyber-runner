//! First-launch runtime installer.
//!
//! Copies the bundled Python runtime (main.py + init/ + run.py + pyproject.toml
//! + deps) from the read-only app bundle / repo into the writable user data
//! directory, where `ensure_deps` then creates the `.venv` and installs
//! `pymobiledevice3==5.0.4`.
//!
//! Why this exists: before vendoring, Cyber Runner relied on the developer
//! manually placing a copy of the legacy `run/` repo at the user runtime path.
//! Anyone cloning cyber-runner without that side-cargo would get a
//! `runtime/main.py: No such file` failure (e.g. issue #1: Windows user).
//! Now `runtime/` ships in the repo and gets bundled, so a fresh install
//! works out of the box.
//!
//! Strategy is intentionally conservative: copy **only when the user runtime
//! is empty / missing `main.py`**. Don't overwrite user-modified `config.yaml`
//! / `HNroute.txt` / etc. on every launch. If the user wants a clean reset
//! they can delete `~/.../CyberRunner/runtime/` and relaunch.

use std::fs;
use std::io;
use std::path::Path;

/// Ensure the user data runtime directory has the Python files needed to run
/// the engine. Copies from `bundled_runtime` (read-only) to `user_runtime`
/// (writable) only on first launch.
///
/// No-op when:
/// - `bundled_runtime` doesn't exist (dev mode without `runtime/` in repo —
///   caller is expected to have set things up manually)
/// - `user_runtime/main.py` already exists (already prepared; preserves user
///   edits to config files)
pub fn ensure_runtime_files(bundled_runtime: &Path, user_runtime: &Path) -> io::Result<bool> {
    if !bundled_runtime.exists() {
        return Ok(false);
    }
    let user_main = user_runtime.join("main.py");
    if user_main.exists() {
        return Ok(false);
    }

    fs::create_dir_all(user_runtime)?;
    copy_dir_recursive(bundled_runtime, user_runtime)?;
    Ok(true)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip vcs, caches, virtualenvs — these should never be in the bundle
        // but guard anyway so a sloppy dev rebuild can't poison user data.
        if name_str.starts_with('.')
            || name_str == "__pycache__"
            || name_str == "node_modules"
        {
            continue;
        }

        let from = entry.path();
        let to = dst.join(&name);
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
        // Symlinks: not expected; skip silently.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_op_when_bundle_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("nonexistent");
        let user = tmp.path().join("user");
        let copied = ensure_runtime_files(&bundle, &user).unwrap();
        assert!(!copied);
        assert!(!user.exists());
    }

    #[test]
    fn no_op_when_user_runtime_already_has_main_py() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("bundle");
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join("main.py"), b"bundled").unwrap();
        let user = tmp.path().join("user");
        fs::create_dir_all(&user).unwrap();
        fs::write(user.join("main.py"), b"user version").unwrap();

        let copied = ensure_runtime_files(&bundle, &user).unwrap();
        assert!(!copied);
        let preserved = fs::read(user.join("main.py")).unwrap();
        assert_eq!(preserved, b"user version", "user file must not be overwritten");
    }

    #[test]
    fn copies_when_user_runtime_missing_main_py() {
        let tmp = tempfile::tempdir().unwrap();
        let bundle = tmp.path().join("bundle");
        fs::create_dir_all(bundle.join("init")).unwrap();
        fs::write(bundle.join("main.py"), b"hello").unwrap();
        fs::write(bundle.join("init").join("init.py"), b"init code").unwrap();
        fs::write(bundle.join("pyproject.toml"), b"[project]").unwrap();
        // Should be skipped:
        fs::create_dir_all(bundle.join("__pycache__")).unwrap();
        fs::write(bundle.join("__pycache__").join("trash.pyc"), b"junk").unwrap();

        let user = tmp.path().join("user");
        let copied = ensure_runtime_files(&bundle, &user).unwrap();
        assert!(copied);
        assert_eq!(fs::read(user.join("main.py")).unwrap(), b"hello");
        assert_eq!(fs::read(user.join("init").join("init.py")).unwrap(), b"init code");
        assert_eq!(fs::read(user.join("pyproject.toml")).unwrap(), b"[project]");
        assert!(!user.join("__pycache__").exists(), "__pycache__ must be skipped");
    }
}
