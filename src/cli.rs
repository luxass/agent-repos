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

