//! 文件 / 目录路径，跨平台等价于 Swift `RunControlPaths`。
//!
//! - macOS: `~/Library/Application Support/cyber-runner/`
//! - Windows: `%LOCALAPPDATA%\cyber-runner\`
//! - Linux: `$XDG_DATA_HOME/cyber-runner/` 或 `~/.local/share/cyber-runner/`

use std::path::{Path, PathBuf};

const APP_DIR: &str = "cyber-runner";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub repository_path: PathBuf,
    pub log_path: PathBuf,
    pub pid_path: PathBuf,
    pub base_path: PathBuf,
}

impl Paths {
    /// 平台默认位置：log/pid 都放在 `base_path` 下，`base_path` 用 `dirs::data_local_dir()`。
    pub fn standard(repository_path: impl AsRef<Path>) -> Self {
        let base = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(APP_DIR);
        Self::with_base(repository_path, base)
    }

    /// 显式指定 `base_path`，主要给测试用。
    pub fn with_base(repository_path: impl AsRef<Path>, base: impl Into<PathBuf>) -> Self {
        let base_path = base.into();
        Self {
            repository_path: repository_path.as_ref().to_path_buf(),
            log_path: base_path.join("run.log"),
            pid_path: base_path.join("run.pid"),
            base_path,
        }
    }

    /// 历史 log 备份路径 `<log>.last`，等价于 Swift helper 里的 `request.logPath + ".last"`。
    pub fn archived_log_path(&self) -> PathBuf {
        let mut p = self.log_path.clone().into_os_string();
        p.push(".last");
        PathBuf::from(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_paths_live_under_base() {
        let p = Paths::standard("/tmp/repo");
        assert!(p.log_path.starts_with(&p.base_path));
        assert!(p.pid_path.starts_with(&p.base_path));
        assert!(p.base_path.ends_with(APP_DIR));
    }

    #[test]
    fn explicit_base_is_respected() {
        let p = Paths::with_base("/tmp/repo", "/var/tmp/test-base");
        assert_eq!(p.base_path, PathBuf::from("/var/tmp/test-base"));
        assert_eq!(p.log_path, PathBuf::from("/var/tmp/test-base/run.log"));
        assert_eq!(p.pid_path, PathBuf::from("/var/tmp/test-base/run.pid"));
    }

    #[test]
    fn archived_log_appends_dot_last() {
        let p = Paths::with_base("/tmp/repo", "/tmp/base");
        assert_eq!(
            p.archived_log_path(),
            PathBuf::from("/tmp/base/run.log.last")
        );
    }
}
