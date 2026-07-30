//! Terminal output. Everything diagnostic goes to stderr so that stdout stays
//! clean for machine-readable output such as `list --json`.

use std::io::{self, IsTerminal};

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
