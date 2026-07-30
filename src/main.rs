//! `agent-repos` — maintain pinned clones of reference repositories so coding
//! agents read a dependency's real source instead of guessing at its API.
//!
//! This binary shells out to the system `git` rather than linking a git
//! implementation. That keeps it dependency-free and small, and inherits the
//! user's SSH keys, credential helpers, proxies and git-lfs for free.

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
agent-repos - maintain pinned clones of reference repositories for coding agents

Usage: agent-repos <command> [options]

Commands:
  init    [--dir DIR] [--target FILE]... [--no-instructions]
      Prepare the current Git repository: write .agent-repos, ignore the
      clone directory, and seed the agent instruction blocks.

  add     <url> [--tag T | --branch B | --commit SHA]
          [--name N] [--path P] [--desc TEXT] [--use TEXT] [--no-sync]
      Add a reference repository pinned to an exact ref. With no ref flag the
      current head commit of the default branch is pinned.

  update  [<name>...] [--all] [--to REF] [--latest] [--yes]
      Re-check a pin, move it to REF, or advance it to the latest.

  restore [--all]
      Clone any manifest entry missing from disk, at its pinned ref.

  remove  <name> [--keep-files] [--yes]
  list    [--json]
  status
  pin     <name>
      Freeze an entry to the commit currently checked out.

  sync    [--target FILE] [--check]
      Refill the agent-repos blocks in AGENTS.md / CLAUDE.md. --check exits 1
      if anything would change.

  completions <fish|bash|zsh>
  help, --help
  version, --version

Examples:
  agent-repos init
  agent-repos add https://github.com/Effect-TS/effect --tag v3.12.0
  agent-repos add https://github.com/owner/repo --branch main
  agent-repos update --all --latest
  agent-repos sync --check
";

fn main() {
    let arg = std::env::args().nth(1);

    match arg.as_deref() {
        Some("version" | "--version" | "-V") => println!("agent-repos {VERSION}"),
        _ => print!("{HELP}"),
    }
}
