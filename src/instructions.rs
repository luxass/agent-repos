//! The generated blocks in AGENTS.md / CLAUDE.md: finding them, filling them,
//! and writing the result back.
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

use crate::files;
use crate::manifest::{Manifest, Repo};
use crate::ui::{self, Error, Result};

const OPEN_PREFIX: &str = "agent-repos:";
const CLOSE_PREFIX: &str = "/agent-repos:";

/// Blocks written into a target file that has none yet.
const DEFAULT_BLOCKS: &[&str] = &["guidance", "repos"];

/// Fields shown by the `repos` block when it does not say otherwise.
const DEFAULT_FIELDS: &[&str] = &["name", "ref", "path", "desc"];

/// Said in the empty case rather than rendering a table with no rows.
const NOTHING_CONFIGURED: &str = "_No reference repositories configured._";

// --- markers --------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
struct Marker<'a> {
    name: &'a str,
    attrs: Vec<(&'a str, &'a str)>,
}

impl Marker<'_> {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| *value)
    }
}

/// Parses `<!-- agent-repos:name key=value -->`, returning `None` for any line
/// that is not one of our markers.
fn parse_open(line: &str) -> Option<Marker<'_>> {
    let rest = html_comment(line)?.strip_prefix(OPEN_PREFIX)?;
    let mut parts = rest.split_whitespace();

    Some(Marker {
        name: parts.next()?,
        attrs: parts
            .filter_map(|part| part.split_once('='))
            .map(|(key, value)| (key, value.trim_matches('"')))
            .collect(),
    })
}

/// Parses `<!-- /agent-repos:name -->`.
fn parse_close(line: &str) -> Option<&str> {
    Some(html_comment(line)?.strip_prefix(CLOSE_PREFIX)?.trim())
}

fn html_comment(line: &str) -> Option<&str> {
    line.trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")
        .map(str::trim)
}

// --- block bodies ---------------------------------------------------------

/// One column a `repos` block can show. Keeping the manifest key, the heading
/// and the accessor together is what stops the accepted set, the headings and
/// the "expected ..." message from drifting apart.
struct Field {
    key: &'static str,
    heading: &'static str,
    read: fn(&Repo) -> &str,
}

const FIELDS: &[Field] = &[
    Field {
        key: "name",
        heading: "Repo",
        read: |repo| &repo.name,
    },
    Field {
        key: "ref",
        heading: "Version",
        read: |repo| &repo.git_ref,
    },
    Field {
        key: "kind",
        heading: "Kind",
        read: |repo| repo.kind.as_str(),
    },
    Field {
        key: "path",
        heading: "Path",
        read: |repo| &repo.path,
    },
    Field {
        key: "url",
        heading: "URL",
        read: |repo| &repo.url,
    },
    Field {
        key: "desc",
        heading: "Purpose",
        read: |repo| repo.desc.as_deref().unwrap_or_default(),
    },
    Field {
        key: "use",
        heading: "Consult for",
        read: |repo| repo.usage.as_deref().unwrap_or_default(),
    },
];

/// Resolves the field names on a marker, rejecting the whole block if any is
/// unknown.
fn resolve(names: &[String]) -> Result<Vec<&'static Field>> {
    names
        .iter()
        .map(|name| {
            FIELDS
                .iter()
                .find(|field| field.key == name)
                .ok_or_else(|| {
                    let known: Vec<&str> = FIELDS.iter().map(|field| field.key).collect();
                    Error::failure(format!(
                        "unknown field `{name}` (expected {})",
                        known.join(", ")
                    ))
                })
        })
        .collect()
}

/// A field's value, with pipes escaped so they cannot split a markdown cell.
fn cell(repo: &Repo, field: &Field) -> String {
    (field.read)(repo).replace('|', "\\|")
}

/// Prose telling an agent how to treat the clone directory.
fn guidance(dir: &str) -> String {
    format!(
        "## Vendored Repositories\n\
         \n\
         This project vendors external repositories under `{dir}/` as read-only \
         reference material for coding agents.\n\
         \n\
         - Prefer examples and patterns from the vendored source code over generated \
         guesses or web search results.\n\
         - Do not edit files under `{dir}/` unless explicitly asked.\n\
         - Do not import from `{dir}/`; application code must continue importing from \
         normal package dependencies.\n\
         - Read each repository's own AGENTS.md, README, and docs before relying on \
         implementation details.\n\
         - Each clone is pinned to the ref recorded in \
         `.agent-repos/manifest.toml`. If one is missing, run \
         `agent-repos restore` rather than cloning it by hand."
    )
}

/// Every entry, as a markdown table or as a bullet list.
fn repos(manifest: &Manifest, fields: &[String], format: &str) -> Result<String> {
    let bulleted = match format {
        "table" => false,
        "list" => true,
        other => {
            return Err(Error::failure(format!(
                "unknown format `{other}` (expected table or list)"
            )));
        }
    };

    // Fields are resolved before the empty check, so a misspelled field is
    // caught even with nothing configured yet rather than lying in wait until
    // the first `add`.
    let fields = resolve(fields)?;
    if manifest.repos.is_empty() {
        return Ok(NOTHING_CONFIGURED.to_string());
    }

    if bulleted {
        // The name leads the bullet, so it is never repeated among the details.
        let rows: Vec<String> = manifest
            .repos
            .iter()
            .map(|repo| {
                let details: Vec<String> = fields
                    .iter()
                    .filter(|field| field.key != "name")
                    .map(|field| (field.heading, cell(repo, field)))
                    .filter(|(_, value)| !value.is_empty())
                    .map(|(heading, value)| format!("{heading}: {value}"))
                    .collect();

                format!("- **{}** — {}", repo.name, details.join(", "))
            })
            .collect();

        return Ok(rows.join("\n"));
    }

    let headings: Vec<&str> = fields.iter().map(|field| field.heading).collect();
    let mut rows = vec![
        format!("| {} |", headings.join(" | ")),
        format!("|{}", "---|".repeat(fields.len())),
    ];

    for repo in &manifest.repos {
        let cells: Vec<String> = fields.iter().map(|field| cell(repo, field)).collect();
        rows.push(format!("| {} |", cells.join(" | ")));
    }

    Ok(rows.join("\n"))
}

fn repo_detail(repo: &Repo) -> String {
    let mut paragraphs = vec![format!(
        "**{}** — pinned to `{}` ({}) at `{}`.",
        repo.name,
        repo.git_ref,
        repo.kind.as_str(),
        repo.path
    )];
    if let Some(desc) = &repo.desc {
        paragraphs.push(desc.clone());
    }
    if let Some(usage) = &repo.usage {
        paragraphs.push(format!("Consult for: {usage}"));
    }
    paragraphs.join("\n\n")
}

/// Produces the body for one block. Returns it without a trailing newline;
/// [`rewrite`] owns the layout.
fn render_block(
    manifest: &Manifest,
    marker: &Marker<'_>,
    file: &str,
    line: usize,
) -> Result<String> {
    let fields: Vec<String> = match marker.attr("fields") {
        Some(list) => list
            .split(',')
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .map(str::to_string)
            .collect(),
        None => DEFAULT_FIELDS
            .iter()
            .map(|field| (*field).to_string())
            .collect(),
    };

    let located = |err: Error| Error::failure(format!("{file}:{line}: {err}"));

    match marker.name {
        "guidance" => Ok(guidance(&manifest.dir)),

        "repos" => {
            repos(manifest, &fields, marker.attr("format").unwrap_or("table")).map_err(located)
        }

        "repo" => {
            let Some(name) = marker.attr("name") else {
                return Err(Error::failure(format!(
                    "{file}:{line}: the `repo` block needs a name, \
                     e.g. <!-- agent-repos:repo name=effect -->"
                )));
            };
            manifest
                .repos
                .iter()
                .find(|repo| repo.name == name)
                .map(repo_detail)
                .ok_or_else(|| Error::failure(format!("{file}:{line}: no entry named `{name}`")))
        }

        "paths" => Ok(manifest
            .repos
            .iter()
            .map(|repo| repo.path.as_str())
            .collect::<Vec<_>>()
            .join("\n")),

        other => Err(Error::failure(format!(
            "{file}:{line}: unknown block `{other}` \
             (expected guidance, repos, repo or paths)"
        ))),
    }
}

// --- rewriting ------------------------------------------------------------

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
        let close = (index + 1..lines.len())
            .find(|&candidate| parse_close(lines[candidate]) == Some(marker.name));

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

/// Computes what a target file should contain.
///
/// A file with no markers at all gains the default set, appended after
/// whatever is already there — which is how `init` seeds a project's existing
/// AGENTS.md without disturbing it.
fn desired(existing: &str, manifest: &Manifest, file: &str) -> Result<String> {
    let mut text = existing.to_string();

    let has_marker = text
        .lines()
        .any(|line| parse_open(line).is_some() || parse_close(line).is_some());

    if !has_marker {
        if !text.is_empty() {
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push('\n');
        }
        let skeleton = DEFAULT_BLOCKS
            .iter()
            .map(|name| format!("<!-- {OPEN_PREFIX}{name} -->\n<!-- {CLOSE_PREFIX}{name} -->\n"))
            .collect::<Vec<_>>()
            .join("\n");
        text.push_str(&skeleton);
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
        files::validate_relative("target", target)?;

        let path = root.join(target);
        let existing = fs::read_to_string(&path).unwrap_or_default();
        let next = desired(&existing, manifest, target)?;

        if next == existing {
            continue;
        }
        drifted = true;

        match mode {
            SyncMode::Check => ui::log(&format!("{target} is out of date")),
            SyncMode::Quiet => files::write_atomic(&path, &next)?,
            SyncMode::Report => {
                files::write_atomic(&path, &next)?;
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

/// Refreshes the generated blocks after a command changes the manifest.
///
/// A problem in a target file — an unknown block, say — must not undo work that
/// already succeeded, so this reports and carries on rather than failing the
/// command. The manifest is already saved; `agent-repos sync` will pick it up
/// once the file is fixed.
pub(crate) fn refresh(root: &Path, manifest: &Manifest) {
    if manifest.targets.is_empty() {
        return;
    }
    if let Err(err) = apply(root, manifest, &manifest.targets, SyncMode::Quiet) {
        ui::error(&format!("could not refresh instruction files: {err}"));
    }
}
