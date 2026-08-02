//! Refilling the generated blocks in AGENTS.md / CLAUDE.md.
//!
//! A block is a pair of HTML comments:
//!
//! ```text
//! <!-- agent-repos:repos fields=name,ref -->
//! ...generated...
//! <!-- /agent-repos:repos -->
//! ```
//!
//! Everything outside a block is left exactly as it was, so these files stay
//! hand-written documents with a few managed regions.
//!
//! Rewriting is idempotent: running sync twice produces byte-identical output.
//! That is what makes `sync --check` usable in a pre-commit hook or CI.

use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::{fsx, render, ui};

const OPEN_PREFIX: &str = "agent-repos:";
const CLOSE_PREFIX: &str = "/agent-repos:";

/// Blocks written into a target file that has none yet.
const DEFAULT_BLOCKS: &[&str] = &["guidance", "repos"];

#[derive(Debug, PartialEq, Eq)]
struct Marker {
    name: String,
    attrs: Vec<(String, String)>,
}

/// Parses `<!-- agent-repos:name key=value -->`, returning `None` for any line
/// that is not one of our markers.
fn parse_open(line: &str) -> Option<Marker> {
    let body = html_comment(line)?;
    let rest = body.strip_prefix(OPEN_PREFIX)?;

    let mut parts = rest.split_whitespace();
    let name = parts.next()?.to_string();

    let attrs = parts
        .filter_map(|part| {
            part.split_once('=')
                .map(|(key, value)| (key.to_string(), value.trim_matches('"').to_string()))
        })
        .collect();

    Some(Marker { name, attrs })
}

/// Parses `<!-- /agent-repos:name -->`.
fn parse_close(line: &str) -> Option<String> {
    let body = html_comment(line)?;
    let name = body.strip_prefix(CLOSE_PREFIX)?;
    Some(name.trim().to_string())
}

fn html_comment(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix("<!--")?
        .strip_suffix("-->")
        .map(str::trim)
}

fn attr<'a>(marker: &'a Marker, key: &str) -> Option<&'a str> {
    marker
        .attrs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

/// Produces the body for one block.
fn render_block(manifest: &Manifest, marker: &Marker, file: &str, line: usize) -> Result<String> {
    let fields: Vec<String> = match attr(marker, "fields") {
        Some(list) => list
            .split(',')
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .map(str::to_string)
            .collect(),
        None => render::DEFAULT_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect(),
    };

    let located = |err: Error| Error::failure(format!("{file}:{line}: {err}"));

    match marker.name.as_str() {
        "guidance" => Ok(render::guidance(&manifest.dir)),

        "repos" => match attr(marker, "format").unwrap_or("table") {
            "table" => render::repos_table(manifest, &fields).map_err(located),
            "list" => render::repos_list(manifest, &fields).map_err(located),
            other => Err(Error::failure(format!(
                "{file}:{line}: unknown format `{other}` (expected table or list)"
            ))),
        },

        "repo" => {
            let Some(name) = attr(marker, "name") else {
                return Err(Error::failure(format!(
                    "{file}:{line}: the `repo` block needs a name, \
                     e.g. <!-- agent-repos:repo name=effect -->"
                )));
            };
            manifest
                .repos
                .iter()
                .find(|repo| repo.name == name)
                .map(render::repo_detail)
                .ok_or_else(|| Error::failure(format!("{file}:{line}: no entry named `{name}`")))
        }

        "paths" => Ok(render::paths(manifest)),

        other => Err(Error::failure(format!(
            "{file}:{line}: unknown block `{other}` \
             (expected guidance, repos, repo or paths)"
        ))),
    }
}

/// Rewrites every block in `text`, leaving everything else untouched.
fn rewrite(text: &str, manifest: &Manifest, file: &str) -> Result<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];

        let Some(marker) = parse_open(line) else {
            // A stray close marker means the file is malformed; better to say
            // so than to silently leave it.
            if let Some(name) = parse_close(line) {
                return Err(Error::failure(format!(
                    "{file}:{}: closing marker for `{name}` with nothing open",
                    index + 1
                )));
            }
            out.push_str(line);
            out.push('\n');
            index += 1;
            continue;
        };

        let open_line = index + 1;
        let close = (index + 1..lines.len()).find(|&candidate| {
            parse_close(lines[candidate]).is_some_and(|name| name == marker.name)
        });

        let Some(close) = close else {
            return Err(Error::failure(format!(
                "{file}:{open_line}: `{}` is never closed \
                 (expected <!-- /{OPEN_PREFIX}{} -->)",
                marker.name, marker.name
            )));
        };

        let body = render_block(manifest, &marker, file, open_line)?;

        out.push_str(line);
        out.push('\n');
        if !body.is_empty() {
            out.push_str(&body);
            out.push('\n');
        }
        out.push_str(lines[close]);
        out.push('\n');

        index = close + 1;
    }

    Ok(out)
}

fn has_any_marker(text: &str) -> bool {
    text.lines()
        .any(|line| parse_open(line).is_some() || parse_close(line).is_some())
}

/// The skeleton appended to a target file that has no blocks yet.
fn default_skeleton() -> String {
    DEFAULT_BLOCKS
        .iter()
        .map(|name| format!("<!-- {OPEN_PREFIX}{name} -->\n<!-- {CLOSE_PREFIX}{name} -->\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Computes what a target file should contain.
fn desired(existing: &str, manifest: &Manifest, file: &str) -> Result<String> {
    let mut text = existing.to_string();

    if !has_any_marker(&text) {
        if !text.is_empty() {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push('\n');
        }
        text.push_str(&default_skeleton());
    }

    rewrite(&text, manifest, file)
}

/// How [`apply`] should treat the targets.
///
/// An enum rather than a pair of booleans, because "check, but quietly" is not
/// a thing and a pair of booleans would let a caller ask for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncMode {
    /// Write changes and name each file touched.
    Report,
    /// Write changes without logging, for the refresh after add/remove/update
    /// where the command reports its own result.
    Quiet,
    /// Change nothing; report what is stale.
    Check,
}

/// Brings every configured target up to date, returning whether anything was
/// — or in [`SyncMode::Check`], would have been — changed.
pub(crate) fn apply(
    root: &Path,
    manifest: &Manifest,
    targets: &[String],
    mode: SyncMode,
) -> Result<bool> {
    let mut drifted = false;

    for target in targets {
        crate::paths::validate_relative("target", target)?;

        let path = root.join(target);
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let next = desired(&existing, manifest, target)?;

        if next == existing {
            continue;
        }
        drifted = true;

        match mode {
            SyncMode::Check => ui::log(&format!("{target} is out of date")),
            SyncMode::Quiet => fsx::write_atomic(&path, &next)?,
            SyncMode::Report => {
                fsx::write_atomic(&path, &next)?;
                ui::log(&format!(
                    "{} {target}",
                    if existing.is_empty() {
                        "created"
                    } else {
                        "updated"
                    }
                ));
            }
        }
    }

    Ok(drifted)
}

pub(crate) fn sync(targets: Vec<String>, mode: SyncMode) -> Result<()> {
    let root = crate::git::root()?;
    let _lock = Manifest::lock(&root)?;
    let manifest = Manifest::load(&root)?;

    let targets = if targets.is_empty() {
        manifest.targets.clone()
    } else {
        targets
    };

    if targets.is_empty() {
        ui::log(
            "no instruction files configured (see `targets` in \
             .agent-repos/manifest.toml)",
        );
        return Ok(());
    }

    let drifted = apply(&root, &manifest, &targets, mode)?;

    if drifted && mode == SyncMode::Check {
        return Err(Error::failure(
            "instruction files are out of date; run `agent-repos sync`",
        ));
    }
    if !drifted {
        ui::log("everything is up to date");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Kind, Repo};

    fn manifest() -> Manifest {
        let mut manifest = Manifest::new("repos".to_string(), vec!["AGENTS.md".to_string()]);
        manifest.repos.push(Repo {
            name: "effect".to_string(),
            url: "https://example.invalid/effect".to_string(),
            git_ref: "v3.12.0".to_string(),
            kind: Kind::Tag,
            path: "repos/effect".to_string(),
            track: None,
            desc: Some("Effect runtime".to_string()),
            usage: Some("API shapes".to_string()),
            comments: Vec::new(),
        });
        manifest
    }

    #[test]
    fn open_markers_parse_with_and_without_attributes() {
        assert_eq!(
            parse_open("<!-- agent-repos:repos -->"),
            Some(Marker {
                name: "repos".to_string(),
                attrs: Vec::new()
            })
        );

        let marker = parse_open("<!-- agent-repos:repos fields=name,ref format=list -->").unwrap();
        assert_eq!(marker.name, "repos");
        assert_eq!(attr(&marker, "fields"), Some("name,ref"));
        assert_eq!(attr(&marker, "format"), Some("list"));

        // Indented markers are still markers.
        assert!(parse_open("   <!-- agent-repos:paths -->").is_some());
        // Unrelated comments are not.
        assert_eq!(parse_open("<!-- just a comment -->"), None);
        assert_eq!(parse_open("not a comment"), None);
    }

    #[test]
    fn close_markers_parse() {
        assert_eq!(
            parse_close("<!-- /agent-repos:repos -->"),
            Some("repos".to_string())
        );
        assert_eq!(parse_close("<!-- agent-repos:repos -->"), None);
    }

    #[test]
    fn content_outside_blocks_is_untouched() {
        let text = "# Title\n\nSome prose.\n\n<!-- agent-repos:paths -->\nstale\n\
                    <!-- /agent-repos:paths -->\n\nMore prose.\n";
        let out = rewrite(text, &manifest(), "AGENTS.md").unwrap();

        assert!(out.starts_with("# Title\n\nSome prose.\n"));
        assert!(out.ends_with("More prose.\n"));
        assert!(out.contains("repos/effect"));
        assert!(!out.contains("stale"), "old content should be replaced");
    }

    #[test]
    fn rewriting_is_idempotent() {
        let text = "<!-- agent-repos:guidance -->\n<!-- /agent-repos:guidance -->\n\n\
                    <!-- agent-repos:repos -->\n<!-- /agent-repos:repos -->\n";
        let once = rewrite(text, &manifest(), "AGENTS.md").unwrap();
        let twice = rewrite(&once, &manifest(), "AGENTS.md").unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn an_empty_block_body_leaves_no_blank_line() {
        let empty = Manifest::new("repos".to_string(), Vec::new());
        let text = "<!-- agent-repos:paths -->\n<!-- /agent-repos:paths -->\n";
        let out = rewrite(text, &empty, "AGENTS.md").unwrap();
        assert_eq!(out, text, "an empty body should render nothing at all");
    }

    #[test]
    fn unclosed_blocks_are_rejected_with_a_location() {
        let text = "intro\n<!-- agent-repos:repos -->\nbody\n";
        let err = rewrite(text, &manifest(), "AGENTS.md").unwrap_err();
        assert!(err.to_string().contains("AGENTS.md:2"), "{err}");
        assert!(err.to_string().contains("never closed"), "{err}");
    }

    #[test]
    fn a_stray_closing_marker_is_rejected() {
        let text = "<!-- /agent-repos:repos -->\n";
        let err = rewrite(text, &manifest(), "AGENTS.md").unwrap_err();
        assert!(err.to_string().contains("nothing open"), "{err}");
    }

    #[test]
    fn unknown_blocks_and_fields_are_rejected() {
        for (text, needle) in [
            (
                "<!-- agent-repos:nope -->\n<!-- /agent-repos:nope -->\n",
                "unknown block `nope`",
            ),
            (
                "<!-- agent-repos:repos fields=nope -->\n<!-- /agent-repos:repos -->\n",
                "unknown field `nope`",
            ),
            (
                "<!-- agent-repos:repos format=yaml -->\n<!-- /agent-repos:repos -->\n",
                "unknown format `yaml`",
            ),
            (
                "<!-- agent-repos:repo -->\n<!-- /agent-repos:repo -->\n",
                "needs a name",
            ),
            (
                "<!-- agent-repos:repo name=absent -->\n<!-- /agent-repos:repo -->\n",
                "no entry named `absent`",
            ),
        ] {
            let err = rewrite(text, &manifest(), "AGENTS.md").unwrap_err();
            assert!(err.to_string().contains(needle), "{text:?}: {err}");
        }
    }

    #[test]
    fn a_file_without_markers_gains_the_default_set() {
        let out = desired("# Project\n", &manifest(), "AGENTS.md").unwrap();

        assert!(out.starts_with("# Project\n\n"), "{out}");
        assert!(out.contains("<!-- agent-repos:guidance -->"));
        assert!(out.contains("<!-- agent-repos:repos -->"));
        assert!(out.contains("Vendored Repositories"));

        // And appending happens only once.
        let again = desired(&out, &manifest(), "AGENTS.md").unwrap();
        assert_eq!(out, again);
    }

    #[test]
    fn a_missing_file_becomes_just_the_blocks() {
        let out = desired("", &manifest(), "AGENTS.md").unwrap();
        assert!(out.starts_with("<!-- agent-repos:guidance -->"), "{out}");
        assert_eq!(desired(&out, &manifest(), "AGENTS.md").unwrap(), out);
    }

    #[test]
    fn a_repo_block_renders_one_entry() {
        let text = "<!-- agent-repos:repo name=effect -->\n<!-- /agent-repos:repo -->\n";
        let out = rewrite(text, &manifest(), "AGENTS.md").unwrap();
        assert!(out.contains("**effect** — pinned to `v3.12.0`"), "{out}");
    }

    #[test]
    fn multiple_blocks_of_the_same_kind_are_all_filled() {
        let text = "<!-- agent-repos:paths -->\n<!-- /agent-repos:paths -->\n\
                    middle\n\
                    <!-- agent-repos:paths -->\n<!-- /agent-repos:paths -->\n";
        let out = rewrite(text, &manifest(), "AGENTS.md").unwrap();
        assert_eq!(out.matches("repos/effect").count(), 2, "{out}");
    }
}
