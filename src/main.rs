//! `agent-repos` — maintain pinned clones of reference repositories so coding
//! agents read a dependency's real source instead of guessing at its API.
//!
//! This binary shells out to the system `git` rather than linking a git
//! implementation. That keeps it dependency-free and small, and inherits the
//! user's SSH keys, credential helpers, proxies and git-lfs for free.

mod args;
mod error;
mod ui;

use args::Parser;
use error::{Error, Result};

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
    let argv: Vec<String> = std::env::args().skip(1).collect();

    if let Err(err) = run(argv) {
        ui::error(&err.to_string());
        std::process::exit(err.code());
    }
}

fn run(argv: Vec<String>) -> Result<()> {
    let mut argv = argv.into_iter();
    let Some(command) = argv.next() else {
        print!("{HELP}");
        return Ok(());
    };
    let rest: Vec<String> = argv.collect();

    match command.as_str() {
        "help" | "--help" | "-h" => {
            print!("{HELP}");
            Ok(())
        }
        "version" | "--version" | "-V" => {
            println!("agent-repos {VERSION}");
            Ok(())
        }
        "init" => init(rest),
        "add" => add(rest),
        "update" => update(rest),
        "restore" => restore(rest),
        "remove" => remove(rest),
        "list" => list(rest),
        "status" => status(rest),
        "pin" => pin(rest),
        "sync" => sync(rest),
        "completions" => completions(rest),
        other => Err(Error::usage(format!(
            "unknown command `{other}` (try `agent-repos help`)"
        ))),
    }
}

/// Until a command lands, report what was parsed and exit non-zero. This keeps
/// the whole surface exercisable — and testable — before any of it does work.
///
/// The option structs below carry `#[expect(dead_code)]` because their fields
/// are only read through `Debug` here, which dead-code analysis deliberately
/// ignores. `expect` rather than `allow`, so each one starts warning the
/// moment the command that consumes it lands and the attribute gets removed
/// rather than lingering.
fn stub(command: &str, options: &dyn std::fmt::Debug) -> Result<()> {
    ui::log(&format!("parsed {command}: {options:?}"));
    Err(Error::unimplemented(command))
}

fn no_positionals(command: &str, rest: &[String]) -> Result<()> {
    match rest.first() {
        Some(first) => Err(Error::usage(format!(
            "`{command}` takes no positional arguments (got `{first}`)"
        ))),
        None => Ok(()),
    }
}

fn exactly_one(command: &str, what: &str, rest: Vec<String>) -> Result<String> {
    let mut rest = rest.into_iter();
    let Some(first) = rest.next() else {
        return Err(Error::usage(format!(
            "`{command}` requires a {what}, e.g. `agent-repos {command} <{what}>`"
        )));
    };
    match rest.next() {
        Some(extra) => Err(Error::usage(format!(
            "`{command}` takes exactly one {what} (got an extra `{extra}`)"
        ))),
        None => Ok(first),
    }
}

/// Which ref an entry is pinned to. Never inferred from a package manifest or
/// lockfile — the user says so explicitly, or the default branch's current
/// head commit is pinned.
#[derive(Debug, PartialEq, Eq)]
enum RefSpec {
    Tag(String),
    Branch(String),
    Commit(String),
    DefaultHead,
}

fn ref_spec(parser: &mut Parser) -> Result<RefSpec> {
    let tag = parser.value("tag", None)?;
    let branch = parser.value("branch", None)?;
    let commit = parser.value("commit", None)?;

    match (tag, branch, commit) {
        (None, None, None) => Ok(RefSpec::DefaultHead),
        (Some(tag), None, None) => Ok(RefSpec::Tag(tag)),
        (None, Some(branch), None) => Ok(RefSpec::Branch(branch)),
        (None, None, Some(commit)) => Ok(RefSpec::Commit(commit)),
        _ => Err(Error::usage(
            "--tag, --branch and --commit are mutually exclusive",
        )),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Shell {
    Fish,
    Bash,
    Zsh,
}

impl Shell {
    fn parse(name: &str) -> Result<Self> {
        match name {
            "fish" => Ok(Self::Fish),
            "bash" => Ok(Self::Bash),
            "zsh" => Ok(Self::Zsh),
            other => Err(Error::usage(format!(
                "unsupported shell `{other}` (expected fish, bash or zsh)"
            ))),
        }
    }
}

#[derive(Debug)]
#[expect(dead_code, reason = "reported via Debug until the command lands")]
struct InitOptions {
    dir: Option<String>,
    targets: Vec<String>,
    no_instructions: bool,
}

fn init(argv: Vec<String>) -> Result<()> {
    let mut parser = Parser::new(argv);
    let options = InitOptions {
        dir: parser.value("dir", None)?,
        targets: parser.values("target", None)?,
        no_instructions: parser.flag("no-instructions", None)?,
    };
    no_positionals("init", &parser.finish()?)?;
    stub("init", &options)
}

#[derive(Debug)]
#[expect(dead_code, reason = "reported via Debug until the command lands")]
struct AddOptions {
    url: String,
    ref_spec: RefSpec,
    name: Option<String>,
    path: Option<String>,
    desc: Option<String>,
    usage: Option<String>,
    no_sync: bool,
}

fn add(argv: Vec<String>) -> Result<()> {
    let mut parser = Parser::new(argv);
    let ref_spec = ref_spec(&mut parser)?;
    let name = parser.value("name", Some('n'))?;
    let path = parser.value("path", Some('p'))?;
    let desc = parser.value("desc", None)?;
    let usage = parser.value("use", None)?;
    let no_sync = parser.flag("no-sync", None)?;
    let url = exactly_one("add", "url", parser.finish()?)?;

    stub(
        "add",
        &AddOptions {
            url,
            ref_spec,
            name,
            path,
            desc,
            usage,
            no_sync,
        },
    )
}

#[derive(Debug)]
#[expect(dead_code, reason = "reported via Debug until the command lands")]
struct UpdateOptions {
    names: Vec<String>,
    all: bool,
    to: Option<String>,
    latest: bool,
    yes: bool,
}

fn update(argv: Vec<String>) -> Result<()> {
    let mut parser = Parser::new(argv);
    let all = parser.flag("all", Some('a'))?;
    let to = parser.value("to", None)?;
    let latest = parser.flag("latest", None)?;
    let yes = parser.flag("yes", Some('y'))?;
    let names = parser.finish()?;

    if all && !names.is_empty() {
        return Err(Error::usage(
            "use either --all or a list of names, not both",
        ));
    }
    if !all && names.is_empty() {
        return Err(Error::usage(
            "`update` needs a name or --all, e.g. `agent-repos update --all`",
        ));
    }
    if to.is_some() && latest {
        return Err(Error::usage("--to and --latest are mutually exclusive"));
    }

    stub(
        "update",
        &UpdateOptions {
            names,
            all,
            to,
            latest,
            yes,
        },
    )
}

#[derive(Debug)]
#[expect(dead_code, reason = "reported via Debug until the command lands")]
struct RestoreOptions {
    all: bool,
}

fn restore(argv: Vec<String>) -> Result<()> {
    let mut parser = Parser::new(argv);
    let options = RestoreOptions {
        all: parser.flag("all", Some('a'))?,
    };
    no_positionals("restore", &parser.finish()?)?;
    stub("restore", &options)
}

#[derive(Debug)]
#[expect(dead_code, reason = "reported via Debug until the command lands")]
struct RemoveOptions {
    name: String,
    keep_files: bool,
    yes: bool,
}

fn remove(argv: Vec<String>) -> Result<()> {
    let mut parser = Parser::new(argv);
    let keep_files = parser.flag("keep-files", None)?;
    let yes = parser.flag("yes", Some('y'))?;
    let name = exactly_one("remove", "name", parser.finish()?)?;

    stub(
        "remove",
        &RemoveOptions {
            name,
            keep_files,
            yes,
        },
    )
}

#[derive(Debug)]
#[expect(dead_code, reason = "reported via Debug until the command lands")]
struct ListOptions {
    json: bool,
}

fn list(argv: Vec<String>) -> Result<()> {
    let mut parser = Parser::new(argv);
    let options = ListOptions {
        json: parser.flag("json", None)?,
    };
    no_positionals("list", &parser.finish()?)?;
    stub("list", &options)
}

#[derive(Debug)]
struct StatusOptions;

fn status(argv: Vec<String>) -> Result<()> {
    let parser = Parser::new(argv);
    no_positionals("status", &parser.finish()?)?;
    stub("status", &StatusOptions)
}

#[derive(Debug)]
#[expect(dead_code, reason = "reported via Debug until the command lands")]
struct PinOptions {
    name: String,
}

fn pin(argv: Vec<String>) -> Result<()> {
    let parser = Parser::new(argv);
    let name = exactly_one("pin", "name", parser.finish()?)?;
    stub("pin", &PinOptions { name })
}

#[derive(Debug)]
#[expect(dead_code, reason = "reported via Debug until the command lands")]
struct SyncOptions {
    targets: Vec<String>,
    check: bool,
}

fn sync(argv: Vec<String>) -> Result<()> {
    let mut parser = Parser::new(argv);
    let options = SyncOptions {
        targets: parser.values("target", None)?,
        check: parser.flag("check", None)?,
    };
    no_positionals("sync", &parser.finish()?)?;
    stub("sync", &options)
}

#[derive(Debug)]
#[expect(dead_code, reason = "reported via Debug until the command lands")]
struct CompletionsOptions {
    shell: Shell,
}

fn completions(argv: Vec<String>) -> Result<()> {
    let parser = Parser::new(argv);
    let shell = exactly_one("completions", "shell", parser.finish()?)?;
    let shell = Shell::parse(&shell)?;
    stub("completions", &CompletionsOptions { shell })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_string()).collect()
    }

    fn spec(args: &[&str]) -> Result<RefSpec> {
        let mut parser = Parser::new(argv(args));
        let spec = ref_spec(&mut parser)?;
        parser.finish()?;
        Ok(spec)
    }

    #[test]
    fn no_ref_flag_pins_the_default_head() {
        assert_eq!(spec(&[]).unwrap(), RefSpec::DefaultHead);
    }

    #[test]
    fn each_ref_flag_is_recognised() {
        assert_eq!(
            spec(&["--tag", "v3.12.0"]).unwrap(),
            RefSpec::Tag("v3.12.0".to_string())
        );
        assert_eq!(
            spec(&["--branch", "main"]).unwrap(),
            RefSpec::Branch("main".to_string())
        );
        assert_eq!(
            spec(&["--commit", "9f3a1c2"]).unwrap(),
            RefSpec::Commit("9f3a1c2".to_string())
        );
    }

    #[test]
    fn ref_flags_are_mutually_exclusive() {
        let err = spec(&["--tag", "v1", "--branch", "main"]).unwrap_err();
        assert_eq!(
            err.to_string(),
            "--tag, --branch and --commit are mutually exclusive"
        );
    }

    #[test]
    fn unknown_command_is_a_usage_error() {
        let err = run(argv(&["frobnicate"])).unwrap_err();
        assert_eq!(err.code(), 2);
        assert!(err.to_string().contains("unknown command `frobnicate`"));
    }

    #[test]
    fn no_arguments_prints_help() {
        assert!(run(Vec::new()).is_ok());
    }

    #[test]
    fn help_and_version_succeed() {
        for command in [
            vec!["help"],
            vec!["--help"],
            vec!["-h"],
            vec!["version"],
            vec!["--version"],
            vec!["-V"],
        ] {
            assert!(run(argv(&command)).is_ok(), "{command:?} should succeed");
        }
    }

    #[test]
    fn every_command_parses_and_reports_unimplemented() {
        let commands = [
            vec!["init"],
            vec!["init", "--dir", "vendor", "--target", "AGENTS.md"],
            vec!["add", "https://example.com/o/r"],
            vec!["add", "https://example.com/o/r", "--tag", "v1.0.0"],
            vec!["update", "--all"],
            vec!["update", "effect"],
            vec!["restore"],
            vec!["remove", "effect", "--yes"],
            vec!["list", "--json"],
            vec!["status"],
            vec!["pin", "effect"],
            vec!["sync", "--check"],
            vec!["completions", "fish"],
        ];

        for command in commands {
            let err = run(argv(&command)).unwrap_err();
            assert_eq!(err.code(), 3, "{command:?} should reach its stub");
            assert!(err.to_string().contains("not implemented yet"));
        }
    }

    #[test]
    fn commands_requiring_an_argument_reject_an_empty_one() {
        for command in [
            vec!["add"],
            vec!["remove"],
            vec!["pin"],
            vec!["completions"],
        ] {
            let err = run(argv(&command)).unwrap_err();
            assert_eq!(err.code(), 2, "{command:?} should be a usage error");
        }
    }

    #[test]
    fn update_rejects_contradictory_targets() {
        let err = run(argv(&["update", "--all", "effect"])).unwrap_err();
        assert_eq!(
            err.to_string(),
            "use either --all or a list of names, not both"
        );

        let err = run(argv(&["update"])).unwrap_err();
        assert!(err.to_string().contains("needs a name or --all"));

        let err = run(argv(&["update", "--all", "--to", "v2", "--latest"])).unwrap_err();
        assert_eq!(err.to_string(), "--to and --latest are mutually exclusive");
    }

    #[test]
    fn unknown_options_are_rejected_per_command() {
        let err = run(argv(&["status", "--nope"])).unwrap_err();
        assert_eq!(err.to_string(), "unknown option --nope");
    }

    #[test]
    fn commands_without_positionals_reject_them() {
        let err = run(argv(&["status", "extra"])).unwrap_err();
        assert!(err.to_string().contains("takes no positional arguments"));
    }

    #[test]
    fn unsupported_shell_is_rejected() {
        let err = run(argv(&["completions", "nushell"])).unwrap_err();
        assert!(err.to_string().contains("unsupported shell `nushell`"));
    }
}
