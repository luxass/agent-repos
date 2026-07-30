//! `agent-repos` — maintain pinned clones of reference repositories so coding
//! agents read a dependency's real source instead of guessing at its API.
//!
//! This binary shells out to the system `git` rather than linking a git
//! implementation. That keeps it dependency-free and small, and inherits the
//! user's SSH keys, credential helpers, proxies and git-lfs for free.
//!
//! Everything here is wiring: [`cli`] turns arguments into a typed call,
//! [`commands`] does the work, and the only thing this file decides is how a
//! failure reaches the shell.

mod args;
mod cli;
mod commands;
mod completions;
mod error;
mod fsx;
mod git;
mod json;
mod manifest;
mod paths;
mod render;
mod sync;
mod ui;
mod version;

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    if let Err(err) = cli::run(argv) {
        ui::error(&err.to_string());
        std::process::exit(err.code());
    }
}
