//! Terminal output. Everything diagnostic goes to stderr so that stdout stays
//! clean for machine-readable output such as `list --json`.

use std::io::{self, IsTerminal, Write};

use crate::error::{Error, Result};

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
