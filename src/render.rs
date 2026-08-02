//! Content for the generated blocks in AGENTS.md / CLAUDE.md.
//!
//! Everything here returns the block body without the surrounding markers, and
//! without a trailing newline: [`crate::sync`] owns the layout.

use crate::error::{Error, Result};
use crate::manifest::{Manifest, Repo};

/// Fields shown by the `repos` block when it does not say otherwise.
pub(crate) const DEFAULT_FIELDS: &[&str] = &["name", "ref", "path", "desc"];

fn heading(field: &str) -> Result<&'static str> {
    Ok(match field {
        "name" => "Repo",
        "ref" => "Version",
        "kind" => "Kind",
        "path" => "Path",
        "url" => "URL",
        "desc" => "Purpose",
        "use" => "Consult for",
        other => {
            return Err(Error::failure(format!(
                "unknown field `{other}` (expected name, ref, kind, path, url, desc or use)"
            )));
        }
    })
}

fn cell(repo: &Repo, field: &str) -> String {
    let value = match field {
        "name" => repo.name.clone(),
        "ref" => repo.git_ref.clone(),
        "kind" => repo.kind.as_str().to_string(),
        "path" => repo.path.clone(),
        "url" => repo.url.clone(),
        "desc" => repo.desc.clone().unwrap_or_default(),
        "use" => repo.usage.clone().unwrap_or_default(),
        _ => String::new(),
    };

    // A pipe in a value would otherwise split the markdown cell.
    value.replace('|', "\\|")
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
    // Validate before the empty check, so a misspelled field is caught even
    // with nothing configured yet rather than lying in wait until the first
    // `add`.
    let headings: Vec<&str> = fields
        .iter()
        .map(|field| heading(field))
        .collect::<Result<_>>()?;

    if manifest.repos.is_empty() {
        return Ok("_No reference repositories configured._".to_string());
    }

    let mut out = format!("| {} |\n", headings.join(" | "));
    out.push_str(&format!("|{}\n", "---|".repeat(fields.len())));

    for repo in &manifest.repos {
        let cells: Vec<String> = fields.iter().map(|field| cell(repo, field)).collect();
        out.push_str(&format!("| {} |\n", cells.join(" | ")));
    }

    Ok(out.trim_end().to_string())
}

pub(crate) fn repos_list(manifest: &Manifest, fields: &[String]) -> Result<String> {
    for field in fields {
        heading(field)?;
    }

    if manifest.repos.is_empty() {
        return Ok("_No reference repositories configured._".to_string());
    }

    let mut out = String::new();
    for repo in &manifest.repos {
        let rest: Vec<String> = fields
            .iter()
            .filter(|field| field.as_str() != "name")
            .map(|field| {
                let value = cell(repo, field);
                if value.is_empty() {
                    String::new()
                } else {
                    format!("{}: {value}", heading(field).unwrap_or(field))
                }
            })
            .filter(|part| !part.is_empty())
            .collect();

        out.push_str(&format!("- **{}** — {}\n", repo.name, rest.join(", ")));
    }

    Ok(out.trim_end().to_string())
}

pub(crate) fn repo_detail(repo: &Repo) -> String {
    let mut out = format!(
        "**{}** — pinned to `{}` ({}) at `{}`.",
        repo.name,
        repo.git_ref,
        repo.kind.as_str(),
        repo.path
    );
    if let Some(desc) = &repo.desc {
        out.push_str(&format!("\n\n{desc}"));
    }
    if let Some(usage) = &repo.usage {
        out.push_str(&format!("\n\nConsult for: {usage}"));
    }
    out
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
