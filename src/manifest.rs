//! The `.agent-repos` manifest: a deliberately small subset of TOML.
//!
//! Supported: `#` comments, top-level `key = value` for strings, integers,
//! booleans and arrays of strings, and `[[repo]]` array-of-tables. Strings are
//! basic strings with `\"`, `\\`, `\n` and `\t` escapes.
//!
//! Not supported, by design: inline tables, multi-line strings, dotted keys,
//! and values spanning lines. Unknown keys are a hard error rather than being
//! silently dropped, so a typo never costs you a pin.
//!
//! Comments in the header, and comments immediately preceding a `[[repo]]`,
//! are preserved across a rewrite. A trailing comment on a `key = value` line
//! is parsed and discarded.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::paths;

pub(crate) const FILE_NAME: &str = ".agent-repos";
pub(crate) const DEFAULT_DIR: &str = "repos";
pub(crate) const DEFAULT_TARGET: &str = "AGENTS.md";

/// Bumped only for a breaking format change.
const FORMAT_VERSION: i64 = 1;

const DEFAULT_HEADER: &[&str] = &[
    "# agent-repos manifest",
    "# Committed on purpose: this is what `agent-repos restore` reproduces.",
];

/// Which kind of ref an entry is pinned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Tag,
    Branch,
    Commit,
}

impl Kind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Tag => "tag",
            Self::Branch => "branch",
            Self::Commit => "commit",
        }
    }

    fn parse(value: &str, line: usize) -> Result<Self> {
        match value {
            "tag" => Ok(Self::Tag),
            "branch" => Ok(Self::Branch),
            "commit" => Ok(Self::Commit),
            other => Err(Error::failure(format!(
                "line {line}: kind must be tag, branch or commit (got {other:?})"
            ))),
        }
    }

    /// A branch is the only kind that moves on its own.
    pub(crate) fn is_pinned(self) -> bool {
        !matches!(self, Self::Branch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Repo {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) git_ref: String,
    pub(crate) kind: Kind,
    pub(crate) path: String,
    /// For a commit pin, the branch the sha came from, so `--latest` knows
    /// what to look at. Never used to move the pin on its own.
    pub(crate) track: Option<String>,
    pub(crate) desc: Option<String>,
    pub(crate) usage: Option<String>,
    /// Comment lines that preceded this entry, preserved on rewrite.
    pub(crate) comments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Manifest {
    pub(crate) dir: String,
    pub(crate) targets: Vec<String>,
    pub(crate) repos: Vec<Repo>,
    header: Vec<String>,
    footer: Vec<String>,
}

impl Manifest {
    pub(crate) fn new(dir: String, targets: Vec<String>) -> Self {
        Self {
            dir,
            targets,
            repos: Vec::new(),
            header: DEFAULT_HEADER.iter().map(|s| (*s).to_string()).collect(),
            footer: Vec::new(),
        }
    }

    pub(crate) fn path(root: &Path) -> PathBuf {
        root.join(FILE_NAME)
    }

    pub(crate) fn load(root: &Path) -> Result<Self> {
        let file = Self::path(root);
        let text = fs::read_to_string(&file).map_err(|err| {
            Error::failure(format!(
                "could not read {}: {err}. Run `agent-repos init` first.",
                file.display()
            ))
        })?;
        Self::parse(&text).map_err(|err| Error::failure(format!("{}: {err}", FILE_NAME)))
    }

    pub(crate) fn save(&self, root: &Path) -> Result<()> {
        crate::fsx::write_atomic(&Self::path(root), &self.render())
    }

    pub(crate) fn parse(text: &str) -> Result<Self> {
        let mut header: Vec<String> = Vec::new();
        let mut pending: Vec<String> = Vec::new();
        let mut version: Option<i64> = None;
        let mut dir: Option<String> = None;
        let mut targets: Option<Vec<String>> = None;
        let mut repos: Vec<Repo> = Vec::new();
        let mut current: Option<PartialRepo> = None;
        let mut body_started = false;

        for (index, raw) in text.lines().enumerate() {
            let line_no = index + 1;
            let line = raw.trim();

            if line.is_empty() {
                continue;
            }

            if line.starts_with('#') {
                if body_started {
                    pending.push(line.to_string());
                } else {
                    header.push(line.to_string());
                }
                continue;
            }

            body_started = true;

            if line == "[[repo]]" {
                if let Some(partial) = current.take() {
                    repos.push(partial.build()?);
                }
                current = Some(PartialRepo::new(std::mem::take(&mut pending), line_no));
                continue;
            }

            if line.starts_with('[') {
                return Err(Error::failure(format!(
                    "line {line_no}: the only supported table is [[repo]] (got {line})"
                )));
            }

            let Some((key, rest)) = line.split_once('=') else {
                return Err(Error::failure(format!(
                    "line {line_no}: expected `key = value` (got {line})"
                )));
            };
            let key = key.trim();
            let value = parse_value(rest.trim(), line_no)?;

            match current.as_mut() {
                Some(partial) => partial.set(key, value, line_no)?,
                None => match key {
                    "version" => version = Some(value.int(key, line_no)?),
                    "dir" => dir = Some(value.string(key, line_no)?),
                    "targets" => targets = Some(value.array(key, line_no)?),
                    other => {
                        return Err(Error::failure(format!(
                            "line {line_no}: unknown key {other:?}"
                        )));
                    }
                },
            }
        }

        if let Some(partial) = current.take() {
            repos.push(partial.build()?);
        }

        match version {
            Some(FORMAT_VERSION) => {}
            Some(other) => {
                return Err(Error::failure(format!(
                    "unsupported version {other}; this build understands version {FORMAT_VERSION}"
                )));
            }
            None => return Err(Error::failure("missing required key `version`")),
        }

        let dir = dir.unwrap_or_else(|| DEFAULT_DIR.to_string());
        paths::validate_relative("dir", &dir)?;

        let manifest = Self {
            dir,
            targets: targets.unwrap_or_else(|| vec![DEFAULT_TARGET.to_string()]),
            repos,
            header,
            footer: pending,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<()> {
        for (index, repo) in self.repos.iter().enumerate() {
            paths::validate_relative("path", &repo.path)?;

            if !paths::is_inside(&self.dir, &repo.path) {
                return Err(Error::failure(format!(
                    "entry `{}` has path {} outside the clone directory {}/",
                    repo.name, repo.path, self.dir
                )));
            }
            if repo.kind != Kind::Commit && repo.track.is_some() {
                return Err(Error::failure(format!(
                    "entry `{}` sets `track`, which only applies to a commit pin",
                    repo.name
                )));
            }
            if let Some(earlier) = self.repos[..index].iter().find(|r| r.name == repo.name) {
                return Err(Error::failure(format!(
                    "duplicate entry name `{}`",
                    earlier.name
                )));
            }
            if let Some(earlier) = self.repos[..index].iter().find(|r| r.path == repo.path) {
                return Err(Error::failure(format!(
                    "entries `{}` and `{}` share the path {}",
                    earlier.name, repo.name, repo.path
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn render(&self) -> String {
        let mut out = String::new();

        for line in &self.header {
            out.push_str(line);
            out.push('\n');
        }
        if !self.header.is_empty() {
            out.push('\n');
        }

        out.push_str(&format!("version = {FORMAT_VERSION}\n"));
        out.push_str(&format!("dir = \"{}\"\n", escape(&self.dir)));
        out.push_str("targets = [");
        for (index, target) in self.targets.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("\"{}\"", escape(target)));
        }
        out.push_str("]\n");

        for repo in &self.repos {
            out.push('\n');
            for comment in &repo.comments {
                out.push_str(comment);
                out.push('\n');
            }
            out.push_str("[[repo]]\n");
            out.push_str(&format!("name = \"{}\"\n", escape(&repo.name)));
            out.push_str(&format!("url = \"{}\"\n", escape(&repo.url)));
            out.push_str(&format!("ref = \"{}\"\n", escape(&repo.git_ref)));
            out.push_str(&format!("kind = \"{}\"\n", repo.kind.as_str()));
            out.push_str(&format!("path = \"{}\"\n", escape(&repo.path)));
            if let Some(track) = &repo.track {
                out.push_str(&format!("track = \"{}\"\n", escape(track)));
            }
            if let Some(desc) = &repo.desc {
                out.push_str(&format!("desc = \"{}\"\n", escape(desc)));
            }
            if let Some(usage) = &repo.usage {
                out.push_str(&format!("use = \"{}\"\n", escape(usage)));
            }
        }

        if !self.footer.is_empty() {
            out.push('\n');
            for comment in &self.footer {
                out.push_str(comment);
                out.push('\n');
            }
        }

        out
    }
}

/// A `[[repo]]` block being filled in, so that a missing key can be reported
/// against the block's own line rather than the end of the file.
#[derive(Debug)]
struct PartialRepo {
    comments: Vec<String>,
    line: usize,
    name: Option<String>,
    url: Option<String>,
    git_ref: Option<String>,
    kind: Option<Kind>,
    path: Option<String>,
    track: Option<String>,
    desc: Option<String>,
    usage: Option<String>,
}

impl PartialRepo {
    fn new(comments: Vec<String>, line: usize) -> Self {
        Self {
            comments,
            line,
            name: None,
            url: None,
            git_ref: None,
            kind: None,
            path: None,
            track: None,
            desc: None,
            usage: None,
        }
    }

    fn set(&mut self, key: &str, value: Value, line: usize) -> Result<()> {
        let duplicate = |key: &str| Error::failure(format!("line {line}: duplicate key {key:?}"));

        match key {
            "name" => {
                if self.name.is_some() {
                    return Err(duplicate(key));
                }
                self.name = Some(value.string(key, line)?);
            }
            "url" => {
                if self.url.is_some() {
                    return Err(duplicate(key));
                }
                self.url = Some(value.string(key, line)?);
            }
            "ref" => {
                if self.git_ref.is_some() {
                    return Err(duplicate(key));
                }
                self.git_ref = Some(value.string(key, line)?);
            }
            "kind" => {
                if self.kind.is_some() {
                    return Err(duplicate(key));
                }
                self.kind = Some(Kind::parse(&value.string(key, line)?, line)?);
            }
            "path" => {
                if self.path.is_some() {
                    return Err(duplicate(key));
                }
                self.path = Some(value.string(key, line)?);
            }
            "track" => self.track = Some(value.string(key, line)?),
            "desc" => self.desc = Some(value.string(key, line)?),
            "use" => self.usage = Some(value.string(key, line)?),
            other => {
                return Err(Error::failure(format!(
                    "line {line}: unknown key {other:?} in [[repo]]"
                )));
            }
        }
        Ok(())
    }

    fn build(self) -> Result<Repo> {
        let line = self.line;
        let required = |field: &str| {
            Error::failure(format!(
                "line {line}: [[repo]] is missing required key `{field}`"
            ))
        };

        Ok(Repo {
            name: self.name.ok_or_else(|| required("name"))?,
            url: self.url.ok_or_else(|| required("url"))?,
            git_ref: self.git_ref.ok_or_else(|| required("ref"))?,
            kind: self.kind.ok_or_else(|| required("kind"))?,
            path: self.path.ok_or_else(|| required("path"))?,
            track: self.track,
            desc: self.desc,
            usage: self.usage,
            comments: self.comments,
        })
    }
}

#[derive(Debug)]
enum Value {
    Str(String),
    Int(i64),
    Bool,
    Array(Vec<String>),
}

impl Value {
    fn kind(&self) -> &'static str {
        match self {
            Self::Str(_) => "a string",
            Self::Int(_) => "an integer",
            Self::Bool => "a boolean",
            Self::Array(_) => "an array",
        }
    }

    fn string(self, key: &str, line: usize) -> Result<String> {
        match self {
            Self::Str(value) => Ok(value),
            other => Err(Error::failure(format!(
                "line {line}: {key} must be a string, got {}",
                other.kind()
            ))),
        }
    }

    fn int(self, key: &str, line: usize) -> Result<i64> {
        match self {
            Self::Int(value) => Ok(value),
            other => Err(Error::failure(format!(
                "line {line}: {key} must be an integer, got {}",
                other.kind()
            ))),
        }
    }

    fn array(self, key: &str, line: usize) -> Result<Vec<String>> {
        match self {
            Self::Array(value) => Ok(value),
            other => Err(Error::failure(format!(
                "line {line}: {key} must be an array of strings, got {}",
                other.kind()
            ))),
        }
    }
}

fn parse_value(input: &str, line: usize) -> Result<Value> {
    if input.starts_with('"') {
        let (value, rest) = scan_string(input, line)?;
        expect_end(rest, line)?;
        return Ok(Value::Str(value));
    }

    if input.starts_with('[') {
        let (value, rest) = scan_array(input, line)?;
        expect_end(rest, line)?;
        return Ok(Value::Array(value));
    }

    let token = input
        .split('#')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();

    match token.as_str() {
        "true" | "false" => Ok(Value::Bool),
        "" => Err(Error::failure(format!("line {line}: missing value"))),
        other => other.parse::<i64>().map(Value::Int).map_err(|_| {
            Error::failure(format!(
                "line {line}: {other:?} is not a string, integer or boolean \
                 (strings must be quoted)"
            ))
        }),
    }
}

fn scan_string(input: &str, line: usize) -> Result<(String, &str)> {
    let mut out = String::new();
    let mut chars = input.char_indices();

    match chars.next() {
        Some((_, '"')) => {}
        _ => return Err(Error::failure(format!("line {line}: expected a string"))),
    }

    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Ok((out, &input[index + 1..])),
            '\\' => {
                let Some((_, escape)) = chars.next() else {
                    return Err(Error::failure(format!(
                        "line {line}: string ends with a dangling backslash"
                    )));
                };
                match escape {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    other => {
                        return Err(Error::failure(format!(
                            "line {line}: unsupported escape \\{other} \
                             (only \\\" \\\\ \\n \\t are supported)"
                        )));
                    }
                }
            }
            other => out.push(other),
        }
    }

    Err(Error::failure(format!("line {line}: unterminated string")))
}

fn scan_array(input: &str, line: usize) -> Result<(Vec<String>, &str)> {
    let mut out = Vec::new();
    let mut cursor = &input[1..];

    loop {
        cursor = cursor.trim_start();

        if let Some(rest) = cursor.strip_prefix(']') {
            return Ok((out, rest));
        }
        if !cursor.starts_with('"') {
            return Err(Error::failure(format!(
                "line {line}: arrays may only contain quoted strings"
            )));
        }

        let (value, rest) = scan_string(cursor, line)?;
        out.push(value);
        cursor = rest.trim_start();

        if let Some(rest) = cursor.strip_prefix(',') {
            cursor = rest;
        } else if let Some(rest) = cursor.strip_prefix(']') {
            return Ok((out, rest));
        } else {
            return Err(Error::failure(format!(
                "line {line}: expected ',' or ']' in array"
            )));
        }
    }
}

fn expect_end(rest: &str, line: usize) -> Result<()> {
    let rest = rest.trim();
    if rest.is_empty() || rest.starts_with('#') {
        Ok(())
    } else {
        Err(Error::failure(format!(
            "line {line}: unexpected trailing text {rest:?}"
        )))
    }
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"# agent-repos manifest
# Committed on purpose: this is what `agent-repos restore` reproduces.

version = 1
dir = "repos"
targets = ["AGENTS.md", "CLAUDE.md"]

# The runtime this service is built on.
[[repo]]
name = "effect"
url = "https://github.com/Effect-TS/effect"
ref = "v3.12.0"
kind = "tag"
path = "repos/effect"
desc = "Effect runtime"
use = "API signatures"

[[repo]]
name = "t3code"
url = "https://github.com/pingdotgg/t3code"
ref = "9f3a1c2e5b7d"
kind = "commit"
path = "repos/t3code"
track = "main"
"#;

    #[test]
    fn parses_a_full_manifest() {
        let manifest = Manifest::parse(SAMPLE).unwrap();

        assert_eq!(manifest.dir, "repos");
        assert_eq!(manifest.targets, vec!["AGENTS.md", "CLAUDE.md"]);
        assert_eq!(manifest.repos.len(), 2);

        let effect = &manifest.repos[0];
        assert_eq!(effect.name, "effect");
        assert_eq!(effect.kind, Kind::Tag);
        assert_eq!(effect.git_ref, "v3.12.0");
        assert_eq!(effect.desc.as_deref(), Some("Effect runtime"));
        assert_eq!(effect.usage.as_deref(), Some("API signatures"));
        assert_eq!(effect.track, None);

        let t3code = &manifest.repos[1];
        assert_eq!(t3code.kind, Kind::Commit);
        assert_eq!(t3code.track.as_deref(), Some("main"));
    }

    #[test]
    fn round_trips_byte_for_byte() {
        let once = Manifest::parse(SAMPLE).unwrap();
        let rendered = once.render();
        let twice = Manifest::parse(&rendered).unwrap();

        assert_eq!(once, twice, "parse(render(x)) must equal x");
        assert_eq!(rendered, twice.render(), "render must be idempotent");
    }

    #[test]
    fn preserves_header_and_entry_comments() {
        let rendered = Manifest::parse(SAMPLE).unwrap().render();
        assert!(rendered.starts_with("# agent-repos manifest\n"));
        assert!(rendered.contains("# The runtime this service is built on.\n[[repo]]"));
    }

    #[test]
    fn preserves_a_trailing_comment_block() {
        let text = "version = 1\n\n# a parting note\n";
        let rendered = Manifest::parse(text).unwrap().render();
        assert!(rendered.contains("# a parting note"));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = Manifest::parse("version = 1\nnope = 2\n").unwrap_err();
        assert!(err.to_string().contains("unknown key \"nope\""), "{err}");

        let err = Manifest::parse("version = 1\n[[repo]]\nnope = \"x\"\n").unwrap_err();
        assert!(err.to_string().contains("unknown key \"nope\""), "{err}");
    }

    #[test]
    fn missing_version_is_rejected() {
        let err = Manifest::parse("dir = \"repos\"\n").unwrap_err();
        assert!(err.to_string().contains("missing required key `version`"));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let err = Manifest::parse("version = 99\n").unwrap_err();
        assert!(err.to_string().contains("unsupported version 99"));
    }

    #[test]
    fn missing_required_repo_key_names_the_block() {
        let err = Manifest::parse("version = 1\n[[repo]]\nname = \"x\"\n").unwrap_err();
        assert!(
            err.to_string().contains("missing required key `url`"),
            "{err}"
        );
    }

    #[test]
    fn duplicate_names_and_paths_are_rejected() {
        let entry = |name: &str, path: &str| {
            format!(
                "[[repo]]\nname = \"{name}\"\nurl = \"u\"\nref = \"r\"\n\
                 kind = \"tag\"\npath = \"{path}\"\n"
            )
        };
        let text = format!(
            "version = 1\n{}{}",
            entry("a", "repos/a"),
            entry("a", "repos/b")
        );
        assert!(
            Manifest::parse(&text)
                .unwrap_err()
                .to_string()
                .contains("duplicate entry name")
        );

        let text = format!(
            "version = 1\n{}{}",
            entry("a", "repos/a"),
            entry("b", "repos/a")
        );
        assert!(
            Manifest::parse(&text)
                .unwrap_err()
                .to_string()
                .contains("share the path")
        );
    }

    #[test]
    fn path_outside_the_clone_directory_is_rejected() {
        let text = "version = 1\ndir = \"repos\"\n[[repo]]\nname = \"a\"\n\
                    url = \"u\"\nref = \"r\"\nkind = \"tag\"\npath = \"elsewhere/a\"\n";
        let err = Manifest::parse(text).unwrap_err();
        assert!(
            err.to_string().contains("outside the clone directory"),
            "{err}"
        );
    }

    #[test]
    fn path_traversal_is_rejected() {
        let text = "version = 1\n[[repo]]\nname = \"a\"\nurl = \"u\"\nref = \"r\"\n\
                    kind = \"tag\"\npath = \"repos/../../evil\"\n";
        assert!(Manifest::parse(text).is_err());
    }

    #[test]
    fn track_requires_a_commit_pin() {
        let text = "version = 1\n[[repo]]\nname = \"a\"\nurl = \"u\"\nref = \"v1\"\n\
                    kind = \"tag\"\npath = \"repos/a\"\ntrack = \"main\"\n";
        let err = Manifest::parse(text).unwrap_err();
        assert!(
            err.to_string().contains("only applies to a commit pin"),
            "{err}"
        );
    }

    #[test]
    fn bad_kind_is_rejected() {
        let text = "version = 1\n[[repo]]\nkind = \"subtree\"\n";
        let err = Manifest::parse(text).unwrap_err();
        assert!(
            err.to_string().contains("must be tag, branch or commit"),
            "{err}"
        );
    }

    #[test]
    fn escapes_survive_a_round_trip() {
        let tricky = "quote \" backslash \\ tab \t newline \n done";
        let mut manifest = Manifest::new("repos".to_string(), vec!["AGENTS.md".to_string()]);
        manifest.repos.push(Repo {
            name: "a".to_string(),
            url: "u".to_string(),
            git_ref: "r".to_string(),
            kind: Kind::Tag,
            path: "repos/a".to_string(),
            track: None,
            desc: Some(tricky.to_string()),
            usage: None,
            comments: Vec::new(),
        });

        let parsed = Manifest::parse(&manifest.render()).unwrap();
        assert_eq!(parsed.repos[0].desc.as_deref(), Some(tricky));
    }

    #[test]
    fn trailing_comments_on_values_are_ignored() {
        let text = "version = 1 # the format version\ndir = \"repos\" # where clones go\n";
        let manifest = Manifest::parse(text).unwrap();
        assert_eq!(manifest.dir, "repos");
    }

    #[test]
    fn unquoted_strings_are_rejected() {
        let err = Manifest::parse("version = 1\ndir = repos\n").unwrap_err();
        assert!(err.to_string().contains("strings must be quoted"), "{err}");
    }

    #[test]
    fn unterminated_string_is_rejected() {
        let err = Manifest::parse("version = 1\ndir = \"repos\n").unwrap_err();
        assert!(err.to_string().contains("unterminated string"), "{err}");
    }

    #[test]
    fn unsupported_toml_constructs_are_rejected() {
        for text in [
            "version = 1\n[table]\n",
            "version = 1\ndir = { a = 1 }\n",
            "version = 1\ntargets = [AGENTS.md]\n",
        ] {
            assert!(
                Manifest::parse(text).is_err(),
                "{text:?} should be rejected"
            );
        }
    }

    #[test]
    fn empty_targets_array_is_allowed() {
        let manifest = Manifest::parse("version = 1\ntargets = []\n").unwrap();
        assert!(manifest.targets.is_empty());
        assert_eq!(Manifest::parse(&manifest.render()).unwrap(), manifest);
    }
}
