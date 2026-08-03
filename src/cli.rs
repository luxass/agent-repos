//! The command line: argument text in, a typed call to the rest of the crate
//! out.
//!
//! Nothing here touches the filesystem or the network, which is what makes the
//! whole surface testable without a repository to run against.
//!
//! The parser is hand-rolled because a dependency like `clap` would cost more
//! than the rest of the binary put together, and this CLI only needs four
//! shapes: `--flag`, `--opt value`, `--opt=value` and clustered shorts (`-ab`).
//! Everything after a bare `--` is a positional. Each arm of [`run`] pulls the
//! options it knows about and then finishes the parser, so an unknown option is
//! an error rather than being silently ignored.

use crate::commands::{self, AddRequest, RefSpec, UpdateRequest};
use crate::instructions::SyncMode;
use crate::ui::{Error, Result};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
agent-repos - maintain pinned clones of reference repositories for coding agents

Usage: agent-repos <command> [options]

Commands:
  init    [--dir DIR] [--target FILE]... [--no-instructions]
      Prepare the current Git repository: write .agent-repos/manifest.toml,
      ignore the clone directory and write lock, and seed the instruction blocks.

  add     <url> [--tag T | --branch B | --commit SHA]
          [--name N] [--path P] [--desc TEXT] [--use TEXT] [--no-sync]
      Add a reference repository pinned to an exact ref. With no ref flag the
      current head commit of the default branch is pinned.

  update  [<name>...] [--all] [--to REF] [--latest] [--yes]
      Re-check a pin, move it to REF, or advance it to the latest.

  restore
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
  agent-repos add github:Effect-TS/effect --tag v3.12.0
  agent-repos add git.example.com:owner/repo --branch main
  agent-repos update --all --latest
  agent-repos sync --check
";

pub(crate) fn run(argv: Vec<String>) -> Result<()> {
    let mut argv = argv.into_iter();

    let Some(command) = argv.next() else {
        print!("{HELP}");
        return Ok(());
    };
    let mut parser = Parser::new(argv.collect());

    match command.as_str() {
        "help" | "--help" | "-h" => {
            print!("{HELP}");
            Ok(())
        }

        "version" | "--version" | "-V" => {
            println!("agent-repos {VERSION}");
            Ok(())
        }

        "init" => {
            let dir = parser.value("dir", None)?;
            let targets = parser.values("target", None)?;
            let no_instructions = parser.flag("no-instructions", None)?;
            parser.no_args("init")?;

            commands::init(dir, targets, no_instructions)
        }

        "add" => {
            let ref_spec = ref_spec(&mut parser)?;
            let name = parser.value("name", Some('n'))?;
            let path = parser.value("path", Some('p'))?;
            let desc = parser.value("desc", None)?;
            let usage = parser.value("use", None)?;
            let no_sync = parser.flag("no-sync", None)?;
            let url = parser.one_arg("add", "url")?;

            commands::add(AddRequest {
                url,
                ref_spec,
                name,
                path,
                desc,
                usage,
                no_sync,
            })
        }

        "update" => {
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

            commands::update(UpdateRequest {
                names,
                all,
                to,
                latest,
                yes,
            })
        }

        "restore" => {
            parser.no_args("restore")?;
            commands::restore()
        }

        "remove" => {
            let keep_files = parser.flag("keep-files", None)?;
            let yes = parser.flag("yes", Some('y'))?;

            commands::remove(parser.one_arg("remove", "name")?, keep_files, yes)
        }

        "list" => {
            let json = parser.flag("json", None)?;
            parser.no_args("list")?;

            commands::list(json)
        }

        "status" => {
            parser.no_args("status")?;
            commands::status()
        }

        "pin" => commands::pin(parser.one_arg("pin", "name")?),

        "sync" => {
            let targets = parser.values("target", None)?;
            let check = parser.flag("check", None)?;
            parser.no_args("sync")?;

            commands::sync(
                targets,
                if check {
                    SyncMode::Check
                } else {
                    SyncMode::Report
                },
            )
        }

        "completions" => commands::completions(&parser.one_arg("completions", "shell")?),

        other => Err(Error::usage(format!(
            "unknown command `{other}` (try `agent-repos help`)"
        ))),
    }
}

/// `--tag`, `--branch` and `--commit` name the same thing three ways, so at
/// most one may be given. None of them means the default branch's current head.
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// `--name` or `--name=value`.
    Long(String, Option<String>),
    /// One character of a `-abc` cluster.
    Short(char),
    /// A positional, or a value consumed by a preceding option.
    Value(String),
}

#[derive(Debug)]
struct Parser {
    tokens: Vec<Token>,
    consumed: Vec<bool>,
}

impl Parser {
    fn new(argv: Vec<String>) -> Self {
        let mut tokens = Vec::with_capacity(argv.len());
        let mut literal = false;

        for arg in argv {
            if literal {
                tokens.push(Token::Value(arg));
            } else if arg == "--" {
                literal = true;
            } else if let Some(body) = arg.strip_prefix("--") {
                match body.split_once('=') {
                    Some((name, value)) => {
                        tokens.push(Token::Long(name.to_string(), Some(value.to_string())));
                    }
                    None => tokens.push(Token::Long(body.to_string(), None)),
                }
            } else if arg.len() > 1 && arg.starts_with('-') {
                tokens.extend(arg[1..].chars().map(Token::Short));
            } else {
                tokens.push(Token::Value(arg));
            }
        }

        let consumed = vec![false; tokens.len()];
        Self { tokens, consumed }
    }

    fn find(&self, long: &str, short: Option<char>) -> Option<usize> {
        (0..self.tokens.len()).find(|&i| {
            !self.consumed[i]
                && match &self.tokens[i] {
                    Token::Long(name, _) => name == long,
                    Token::Short(ch) => short == Some(*ch),
                    Token::Value(_) => false,
                }
        })
    }

    /// A boolean flag. Passing a value to one (`--all=yes`) is an error rather
    /// than being quietly dropped.
    fn flag(&mut self, long: &str, short: Option<char>) -> Result<bool> {
        let Some(i) = self.find(long, short) else {
            return Ok(false);
        };
        if let Token::Long(name, Some(_)) = &self.tokens[i] {
            return Err(Error::usage(format!("--{name} does not take a value")));
        }
        self.consumed[i] = true;
        Ok(true)
    }

    /// An option taking one value, given either as `--opt=value` or as
    /// `--opt value`.
    fn value(&mut self, long: &str, short: Option<char>) -> Result<Option<String>> {
        let Some(i) = self.find(long, short) else {
            return Ok(None);
        };

        if let Token::Long(_, Some(value)) = &self.tokens[i] {
            let value = value.clone();
            self.consumed[i] = true;
            return Ok(Some(value));
        }

        // Take the next token, but only if it is an unconsumed positional.
        // This makes `--desc --tag v1` an error instead of silently setting
        // desc to "--tag".
        let next = i + 1;
        if next < self.tokens.len()
            && !self.consumed[next]
            && let Token::Value(value) = &self.tokens[next]
        {
            let value = value.clone();
            self.consumed[i] = true;
            self.consumed[next] = true;
            return Ok(Some(value));
        }

        Err(Error::usage(format!("--{long} requires a value")))
    }

    /// A repeatable option, such as `--target AGENTS.md --target CLAUDE.md`.
    fn values(&mut self, long: &str, short: Option<char>) -> Result<Vec<String>> {
        let mut out = Vec::new();
        while let Some(value) = self.value(long, short)? {
            out.push(value);
        }
        Ok(out)
    }

    /// Returns the positionals in order, and errors on any option the command
    /// did not ask for.
    fn finish(self) -> Result<Vec<String>> {
        let mut positionals = Vec::new();

        for (i, token) in self.tokens.iter().enumerate() {
            if self.consumed[i] {
                continue;
            }
            match token {
                Token::Value(value) => positionals.push(value.clone()),
                Token::Long(name, _) => {
                    return Err(Error::usage(format!("unknown option --{name}")));
                }
                Token::Short(ch) => return Err(Error::usage(format!("unknown option -{ch}"))),
            }
        }

        Ok(positionals)
    }

    /// [`Parser::finish`] for a command that takes no positionals.
    fn no_args(self, command: &str) -> Result<()> {
        match self.finish()?.first() {
            Some(first) => Err(Error::usage(format!(
                "`{command}` takes no positional arguments (got `{first}`)"
            ))),
            None => Ok(()),
        }
    }

    /// [`Parser::finish`] for a command that takes exactly one positional.
    fn one_arg(self, command: &str, what: &str) -> Result<String> {
        let mut positionals = self.finish()?.into_iter();

        let Some(first) = positionals.next() else {
            return Err(Error::usage(format!(
                "`{command}` requires a {what}, e.g. `agent-repos {command} <{what}>`"
            )));
        };

        match positionals.next() {
            Some(extra) => Err(Error::usage(format!(
                "`{command}` takes exactly one {what} (got an extra `{extra}`)"
            ))),
            None => Ok(first),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_string()).collect()
    }

    fn parser(args: &[&str]) -> Parser {
        Parser::new(argv(args))
    }

    fn spec(args: &[&str]) -> Result<RefSpec> {
        let mut parser = parser(args);
        let spec = ref_spec(&mut parser)?;
        parser.finish()?;
        Ok(spec)
    }

    // --- parser ---

    #[test]
    fn separate_value() {
        let mut p = parser(&["--tag", "v3.12.0"]);
        assert_eq!(p.value("tag", None).unwrap().as_deref(), Some("v3.12.0"));
        assert!(p.finish().unwrap().is_empty());
    }

    #[test]
    fn inline_value() {
        let mut p = parser(&["--tag=v3.12.0"]);
        assert_eq!(p.value("tag", None).unwrap().as_deref(), Some("v3.12.0"));
        assert!(p.finish().unwrap().is_empty());
    }

    #[test]
    fn empty_inline_value_is_kept() {
        let mut p = parser(&["--desc="]);
        assert_eq!(p.value("desc", None).unwrap().as_deref(), Some(""));
    }

    #[test]
    fn missing_option_is_none() {
        let mut p = parser(&[]);
        assert_eq!(p.value("tag", None).unwrap(), None);
        assert!(!p.flag("all", None).unwrap());
    }

    #[test]
    fn short_flags_cluster() {
        let mut p = parser(&["-ay"]);
        assert!(p.flag("all", Some('a')).unwrap());
        assert!(p.flag("yes", Some('y')).unwrap());
        assert!(p.finish().unwrap().is_empty());
    }

    #[test]
    fn positionals_keep_order() {
        let mut p = parser(&["one", "--all", "two", "three"]);
        assert!(p.flag("all", None).unwrap());
        assert_eq!(p.finish().unwrap(), vec!["one", "two", "three"]);
    }

    #[test]
    fn repeatable_option() {
        let mut p = parser(&["--target", "AGENTS.md", "--target=CLAUDE.md"]);
        assert_eq!(
            p.values("target", None).unwrap(),
            vec!["AGENTS.md", "CLAUDE.md"]
        );
        assert!(p.finish().unwrap().is_empty());
    }

    #[test]
    fn double_dash_makes_everything_positional() {
        let mut p = parser(&["--all", "--", "--not-an-option", "-x"]);
        assert!(p.flag("all", None).unwrap());
        assert_eq!(p.finish().unwrap(), vec!["--not-an-option", "-x"]);
    }

    #[test]
    fn unknown_long_option_is_rejected() {
        let err = parser(&["--nope"]).finish().unwrap_err();
        assert_eq!(err.to_string(), "unknown option --nope");
    }

    #[test]
    fn unknown_short_option_is_rejected() {
        let err = parser(&["-z"]).finish().unwrap_err();
        assert_eq!(err.to_string(), "unknown option -z");
    }

    #[test]
    fn option_without_value_is_rejected() {
        let mut p = parser(&["--tag"]);
        let err = p.value("tag", None).unwrap_err();
        assert_eq!(err.to_string(), "--tag requires a value");
    }

    #[test]
    fn option_does_not_swallow_the_next_option() {
        let mut p = parser(&["--desc", "--tag", "v1"]);
        assert!(p.value("desc", None).is_err());
    }

    #[test]
    fn flag_given_a_value_is_rejected() {
        let mut p = parser(&["--all=yes"]);
        let err = p.flag("all", None).unwrap_err();
        assert_eq!(err.to_string(), "--all does not take a value");
    }

    #[test]
    fn lone_dash_is_positional() {
        assert_eq!(parser(&["-"]).finish().unwrap(), vec!["-"]);
    }

    #[test]
    fn value_may_look_like_a_path() {
        let mut p = parser(&["--path", "repos/effect"]);
        assert_eq!(
            p.value("path", None).unwrap().as_deref(),
            Some("repos/effect")
        );
    }

    #[test]
    fn positional_counts_are_enforced() {
        assert!(parser(&[]).no_args("status").is_ok());
        assert!(
            parser(&["extra"])
                .no_args("status")
                .unwrap_err()
                .to_string()
                .contains("takes no positional arguments")
        );

        assert_eq!(
            parser(&["effect"]).one_arg("pin", "name").unwrap(),
            "effect"
        );
        assert!(
            parser(&[])
                .one_arg("pin", "name")
                .unwrap_err()
                .to_string()
                .contains("requires a name")
        );
        assert!(
            parser(&["a", "b"])
                .one_arg("pin", "name")
                .unwrap_err()
                .to_string()
                .contains("takes exactly one name")
        );
    }

    // --- dispatch ---

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
