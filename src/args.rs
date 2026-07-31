//! Hand-rolled argument parsing.
//!
//! A dependency like `clap` would cost more than the rest of the binary put
//! together, and this CLI only needs four shapes: `--flag`, `--opt value`,
//! `--opt=value` and clustered shorts (`-ab`). Everything after a bare `--` is
//! a positional.
//!
//! Commands pull the options they know about, then call [`Parser::finish`],
//! which returns the positionals and rejects anything left over. That way an
//! unknown option is an error rather than being silently ignored.

use crate::error::{Error, Result};

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
pub(crate) struct Parser {
    tokens: Vec<Token>,
    consumed: Vec<bool>,
}

impl Parser {
    pub(crate) fn new(argv: Vec<String>) -> Self {
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
    pub(crate) fn flag(&mut self, long: &str, short: Option<char>) -> Result<bool> {
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
    pub(crate) fn value(&mut self, long: &str, short: Option<char>) -> Result<Option<String>> {
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
    pub(crate) fn values(&mut self, long: &str, short: Option<char>) -> Result<Vec<String>> {
        let mut out = Vec::new();
        while let Some(value) = self.value(long, short)? {
            out.push(value);
        }
        Ok(out)
    }

    /// Returns the positionals in order, and errors on any option the command
    /// did not ask for.
    pub(crate) fn finish(self) -> Result<Vec<String>> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser(args: &[&str]) -> Parser {
        Parser::new(args.iter().map(|s| (*s).to_string()).collect())
    }

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
        let p = parser(&["--nope"]);
        let err = p.finish().unwrap_err();
        assert_eq!(err.to_string(), "unknown option --nope");
    }

    #[test]
    fn unknown_short_option_is_rejected() {
        let p = parser(&["-z"]);
        let err = p.finish().unwrap_err();
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
        let p = parser(&["-"]);
        assert_eq!(p.finish().unwrap(), vec!["-"]);
    }

    #[test]
    fn value_may_look_like_a_path() {
        let mut p = parser(&["--path", "repos/effect"]);
        assert_eq!(
            p.value("path", None).unwrap().as_deref(),
            Some("repos/effect")
        );
    }
}
