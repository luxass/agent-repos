//! Content for the generated blocks in AGENTS.md / CLAUDE.md.
//!
//! Everything here returns the block body without the surrounding markers, and
//! without a trailing newline: [`crate::sync`] owns the layout.

use crate::error::{Error, Result};
use crate::manifest::{Manifest, Repo};

/// Fields shown by the `repos` block when it does not say otherwise.
pub(crate) const DEFAULT_FIELDS: &[&str] = &["name", "ref", "path", "desc"];

/// Said in the empty case rather than rendering a table with no rows.
const NOTHING_CONFIGURED: &str = "_No reference repositories configured._";

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
/// unknown. Callers do this before the empty check, so a misspelled field is
/// caught even with nothing configured yet rather than lying in wait until the
/// first `add`.
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
pub(crate) fn guidance(dir: &str) -> String {
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

pub(crate) fn repos_table(manifest: &Manifest, fields: &[String]) -> Result<String> {
    let fields = resolve(fields)?;
    if manifest.repos.is_empty() {
        return Ok(NOTHING_CONFIGURED.to_string());
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

pub(crate) fn repos_list(manifest: &Manifest, fields: &[String]) -> Result<String> {
    let fields = resolve(fields)?;
    if manifest.repos.is_empty() {
        return Ok(NOTHING_CONFIGURED.to_string());
    }

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

    Ok(rows.join("\n"))
}

pub(crate) fn repo_detail(repo: &Repo) -> String {
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

pub(crate) fn paths(manifest: &Manifest) -> String {
    manifest
        .repos
        .iter()
        .map(|repo| repo.path.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Kind;

    fn repo(name: &str) -> Repo {
        Repo {
            name: name.to_string(),
            url: format!("https://example.invalid/{name}"),
            git_ref: "v1.0.0".to_string(),
            kind: Kind::Tag,
            path: format!("repos/{name}"),
            track: None,
            desc: Some(format!("{name} runtime")),
            usage: Some("API shapes".to_string()),
            comments: Vec::new(),
        }
    }

    fn manifest(repos: Vec<Repo>) -> Manifest {
        let mut manifest = Manifest::new("repos".to_string(), vec!["AGENTS.md".to_string()]);
        manifest.repos = repos;
        manifest
    }

    fn fields(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn table_has_a_row_per_repo() {
        let table = repos_table(
            &manifest(vec![repo("a"), repo("b")]),
            &fields(DEFAULT_FIELDS),
        )
        .unwrap();

        assert!(table.starts_with("| Repo | Version | Path | Purpose |"));
        assert_eq!(table.lines().count(), 4, "header, rule, two rows");
        assert!(
            !table.ends_with('\n'),
            "the caller owns the trailing newline"
        );
    }

    #[test]
    fn table_escapes_pipes() {
        let mut entry = repo("a");
        entry.desc = Some("does a | b".to_string());
        let table = repos_table(&manifest(vec![entry]), &fields(&["name", "desc"])).unwrap();
        assert!(table.contains("does a \\| b"), "{table}");
    }

    #[test]
    fn an_unknown_field_is_rejected() {
        let err = repos_table(&manifest(vec![repo("a")]), &fields(&["nope"])).unwrap_err();
        assert!(err.to_string().contains("unknown field `nope`"), "{err}");
    }

    #[test]
    fn an_unknown_field_is_rejected_even_with_no_entries() {
        // Otherwise a typo in `fields=` sits unnoticed until the first add.
        for render in [repos_table, repos_list] {
            let err = render(&manifest(Vec::new()), &fields(&["nope"])).unwrap_err();
            assert!(err.to_string().contains("unknown field `nope`"), "{err}");
        }
    }

    #[test]
    fn empty_manifests_say_so_rather_than_rendering_an_empty_table() {
        assert!(
            repos_table(&manifest(Vec::new()), &fields(DEFAULT_FIELDS))
                .unwrap()
                .contains("No reference repositories")
        );
        assert!(
            repos_list(&manifest(Vec::new()), &fields(DEFAULT_FIELDS))
                .unwrap()
                .contains("No reference repositories")
        );
        assert_eq!(paths(&manifest(Vec::new())), "");
    }

    #[test]
    fn list_format_leads_with_the_name() {
        let list = repos_list(&manifest(vec![repo("effect")]), &fields(DEFAULT_FIELDS)).unwrap();
        assert!(list.starts_with("- **effect** — "), "{list}");
        assert!(list.contains("Version: v1.0.0"), "{list}");
    }

    #[test]
    fn detail_mentions_the_pin_and_the_reason() {
        let detail = repo_detail(&repo("effect"));
        assert!(detail.contains("pinned to `v1.0.0` (tag) at `repos/effect`"));
        assert!(detail.contains("effect runtime"));
        assert!(detail.contains("Consult for: API shapes"));
    }

    #[test]
    fn guidance_names_the_configured_directory() {
        let text = guidance("vendor");
        assert!(text.starts_with("## Vendored Repositories"));
        assert!(text.contains("`vendor/`"));
        assert!(
            !text.contains("under `repos/`"),
            "should not hardcode the default"
        );
        assert!(text.contains("Do not edit files under"));
        assert!(text.contains("Do not import from"));
    }

    #[test]
    fn paths_are_newline_separated() {
        assert_eq!(
            paths(&manifest(vec![repo("a"), repo("b")])),
            "repos/a\nrepos/b"
        );
    }
}
