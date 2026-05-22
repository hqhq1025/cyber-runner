//! PID 文件解析，跨平台等价于 Swift `RunState`。

/// 从 PID 文件内容中读出第一行有效正整数 PID。
pub fn running_pid_from(contents: &str) -> Option<i32> {
    contents
        .lines()
        .next()?
        .trim()
        .parse::<i32>()
        .ok()
        .filter(|&v| v > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_pid() {
        assert_eq!(running_pid_from("12345\n"), Some(12345));
    }

    #[test]
    fn parses_pid_without_newline() {
        assert_eq!(running_pid_from("789"), Some(789));
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(running_pid_from(""), None);
    }

    #[test]
    fn rejects_only_whitespace() {
        assert_eq!(running_pid_from("   \n"), None);
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(running_pid_from("abc"), None);
    }

    #[test]
    fn rejects_zero() {
        assert_eq!(running_pid_from("0"), None);
    }

    #[test]
    fn rejects_negative() {
        assert_eq!(running_pid_from("-1"), None);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(running_pid_from("  789  \n"), Some(789));
    }

    #[test]
    fn multiline_takes_first_line() {
        assert_eq!(running_pid_from("100\n200\n"), Some(100));
    }

    #[test]
    fn ignores_garbage_after_pid_on_first_line_only() {
        assert_eq!(running_pid_from("100abc\n200"), None);
    }
}
