//! macOS privilege escalation via `osascript`.
//!
//! We invoke `/usr/bin/osascript -e 'do shell script "..." with administrator
//! privileges'`. macOS pops a single Touch ID / password prompt and the
//! resulting shell runs as root. The spawned child therefore inherits root
//! privileges without us having to install a long-lived helper.
//!
//! The shell command embedded in the AppleScript literal goes through *two*
//! parsers: AppleScript first (which interprets `\\`, `\"`, `\n`, ...), then
//! `/bin/sh` (which expands `$`, `!`, `~`, globs, ...). We defeat both by
//! single-quoting every token at the shell level and then escaping any `\` or
//! `"` characters that those single-quote sequences introduce for AppleScript.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

/// Spawn `cmd args...` as root via an AppleScript privilege prompt.
///
/// Returns the PID of the spawned `osascript` helper (not the eventual root
/// child — AppleScript hides that). Callers that need to track the inner
/// process should write a PID file from the child itself.
///
/// Triggers a GUI password / Touch ID prompt, so this MUST NOT be called from
/// automated tests.
pub fn spawn_privileged(cmd: &Path, args: &[&OsStr]) -> std::io::Result<u32> {
    let shell_cmd = build_shell_command(cmd, args);
    let escaped = applescript_escape(&shell_cmd);
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        escaped
    );
    let child = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .spawn()?;
    Ok(child.id())
}

/// Wrap a single shell token in single quotes, escaping embedded single
/// quotes via the canonical `'\''` sequence.
fn shell_escape(token: &str) -> String {
    let mut out = String::with_capacity(token.len() + 2);
    out.push('\'');
    for ch in token.chars() {
        if ch == '\'' {
            // close, escaped single quote, reopen
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Join `cmd` and `args` into a single shell-safe command line.
fn build_shell_command(cmd: &Path, args: &[&OsStr]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(args.len() + 1);
    parts.push(shell_escape(&cmd.to_string_lossy()));
    for a in args {
        parts.push(shell_escape(&a.to_string_lossy()));
    }
    parts.join(" ")
}

/// Escape a string for embedding inside an AppleScript double-quoted literal.
/// AppleScript only recognises `\\`, `\"`, `\n`, `\r`, `\t` inside `"..."`.
/// Our shell-escaped tokens contain `\` (from the `'\''` sequence) but never
/// `"`, `\n`, `\r`, or `\t` (we don't accept those), so escaping `\` and `"`
/// is sufficient and correct.
fn applescript_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn shell_escape_plain() {
        assert_eq!(shell_escape("hello"), "'hello'");
    }

    #[test]
    fn shell_escape_with_spaces() {
        assert_eq!(shell_escape("a b c"), "'a b c'");
    }

    #[test]
    fn shell_escape_with_single_quote() {
        // single quote becomes '\''
        assert_eq!(shell_escape("it's"), r"'it'\''s'");
    }

    #[test]
    fn shell_escape_with_double_quote() {
        // double quotes are inert inside single-quote shell strings
        assert_eq!(shell_escape("say \"hi\""), "'say \"hi\"'");
    }

    #[test]
    fn shell_escape_with_backslash() {
        // backslash is literal inside shell single quotes
        assert_eq!(shell_escape(r"C:\path"), r"'C:\path'");
    }

    #[test]
    fn shell_escape_with_dollar_and_bang() {
        // both must be literal — single quotes prevent shell expansion
        assert_eq!(shell_escape("$HOME!"), "'$HOME!'");
    }

    #[test]
    fn shell_escape_empty() {
        assert_eq!(shell_escape(""), "''");
    }

    #[test]
    fn applescript_escape_backslash_doubles() {
        assert_eq!(applescript_escape(r"\"), r"\\");
    }

    #[test]
    fn applescript_escape_double_quote() {
        assert_eq!(applescript_escape("\""), "\\\"");
    }

    #[test]
    fn applescript_escape_passthrough() {
        assert_eq!(applescript_escape("plain $HOME!"), "plain $HOME!");
    }

    #[test]
    fn build_shell_command_joins_with_spaces() {
        let cmd = PathBuf::from("/bin/echo");
        let a = OsStr::new("hello world");
        let b = OsStr::new("done");
        let line = build_shell_command(&cmd, &[a, b]);
        assert_eq!(line, "'/bin/echo' 'hello world' 'done'");
    }

    #[test]
    fn build_shell_command_handles_quotes_and_dollar() {
        let cmd = PathBuf::from("/usr/bin/env");
        let arg = OsStr::new("$PATH it's \"weird\"");
        let line = build_shell_command(&cmd, &[arg]);
        // single quote splits the token; $ and " stay literal under sh
        assert_eq!(line, "'/usr/bin/env' '$PATH it'\\''s \"weird\"'");
    }

    #[test]
    fn full_pipeline_escapes_backslash_for_applescript() {
        // Path with a literal backslash → shell sees `\`, AppleScript sees `\\`
        let cmd = PathBuf::from(r"C:\weird\path");
        let line = build_shell_command(&cmd, &[]);
        let osa = applescript_escape(&line);
        // shell layer: 'C:\weird\path'
        // applescript layer doubles each backslash
        assert_eq!(osa, r"'C:\\weird\\path'");
    }

    #[test]
    fn full_pipeline_handles_quote_in_arg() {
        // arg with single quote should produce the canonical '\'' sequence,
        // and the backslash in that sequence must be doubled for AppleScript.
        let cmd = PathBuf::from("/bin/echo");
        let arg = OsStr::new("it's");
        let line = build_shell_command(&cmd, &[arg]);
        let osa = applescript_escape(&line);
        assert_eq!(osa, r"'/bin/echo' 'it'\\''s'");
    }
}
