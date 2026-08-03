//! How the program talks to the user: the error it fails with, and the
//! terminal output it writes on the way.
//!
//! Everything diagnostic goes to stderr so that stdout stays clean for
//! machine-readable output such as `list --json`.

use std::fmt;
use std::io::{self, IsTerminal, Write};

/// Process exit codes. `0` is returned implicitly on success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitCode {
    /// The command was well-formed but could not be carried out: a malformed
    /// manifest, a git failure, a missing repository.
    Failure = 1,
    /// The command line itself was wrong: unknown option, missing value,
    /// mutually exclusive flags.
    Usage = 2,
}

#[derive(Debug)]
pub(crate) struct Error {
    message: String,
    code: ExitCode,
}

impl Error {
    pub(crate) fn failure(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: ExitCode::Failure,
        }
    }

    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: ExitCode::Usage,
        }
    }

    pub(crate) fn code(&self) -> i32 {
        self.code as i32
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

const RED: &str = "\x1b[31m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// Colour is used only on a real terminal, and never when `NO_COLOR` is set.
/// See <https://no-color.org>.
fn color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    io::stderr().is_terminal()
}

pub(crate) fn log(message: &str) {
    if color() {
        eprintln!("{DIM}agent-repos:{RESET} {message}");
    } else {
        eprintln!("agent-repos: {message}");
    }
}

pub(crate) fn error(message: &str) {
    if color() {
        eprintln!("{RED}agent-repos: error:{RESET} {message}");
    } else {
        eprintln!("agent-repos: error: {message}");
    }
}

/// Asks before doing something destructive.
///
/// With no terminal to ask on — a script, a hook, CI — this is an error rather
/// than a silent yes, so an unattended run can never delete something nobody
/// agreed to. `--yes` is the way to say so up front.
pub(crate) fn confirm(prompt: &str, assume_yes: bool) -> Result<bool> {
    if assume_yes {
        return Ok(true);
    }

    if !io::stdin().is_terminal() {
        return Err(Error::usage(format!(
            "{prompt} Pass --yes to confirm without a terminal."
        )));
    }

    eprint!("agent-repos: {prompt} [y/N] ");
    let _ = io::stderr().flush();

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|err| Error::failure(format!("could not read a reply: {err}")))?;

    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
