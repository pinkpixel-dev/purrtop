//! Running a command from the Run new task dialog.

use std::process::{Command, Stdio};

use super::autostart::find_program;

/// The program a command line starts: its first word, skipping `NAME=value`
/// settings in front of it. A quoted first word keeps its spaces.
pub(super) fn program(command: &str) -> Option<&str> {
    let mut rest = command.trim_start();
    while !rest.is_empty() {
        let (word, after) = match rest.chars().next() {
            Some(quote @ ('"' | '\'')) => match rest[1..].find(quote) {
                Some(end) => (&rest[1..=end], &rest[end + 2..]),
                None => (&rest[1..], ""),
            },
            _ => rest.split_once(char::is_whitespace).unwrap_or((rest, "")),
        };
        let setting = word.split_once('=').is_some_and(|(name, _)| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        if !setting {
            return Some(word);
        }
        rest = after.trim_start();
    }
    None
}

/// Runs `command` through `sh` in its own session, detached from purrtop, so
/// it keeps running after purrtop closes. Checks that the program exists
/// first, since a detached shell can't report that back.
pub fn run(command: &str) -> Result<(), String> {
    let command = command.trim();
    let program = program(command).ok_or("Type a command to run")?;
    let expanded = match program.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
        None => program.to_string(),
    };
    if find_program(&expanded).is_none() {
        return Err(format!("Couldn't find \"{program}\""));
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/".into());
    // `setsid -f` forks and returns right away, so nothing is left to wait on.
    let status = Command::new("setsid")
        .args(["-f", "sh", "-c", command])
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| format!("Couldn't start it: {err}"))?;
    if status.success() { Ok(()) } else { Err("Couldn't start it".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_program_in_a_command_line() {
        assert_eq!(program("firefox --new-window"), Some("firefox"));
        assert_eq!(program("  GDK_BACKEND=wayland gedit notes.txt"), Some("gedit"));
        assert_eq!(program("\"/opt/My App/app\" --flag"), Some("/opt/My App/app"));
        assert_eq!(program("'unterminated"), Some("unterminated"));
        assert_eq!(program("/usr/bin/env FOO=1 thing"), Some("/usr/bin/env"));
        assert_eq!(program("   "), None);
    }

    #[test]
    fn refuses_programs_that_dont_exist() {
        assert_eq!(run(""), Err("Type a command to run".into()));
        assert_eq!(run("purrtop-no-such-program --flag"), Err("Couldn't find \"purrtop-no-such-program\"".into()));
    }

    #[test]
    fn runs_a_real_command() {
        assert_eq!(run("true"), Ok(()));
    }
}
