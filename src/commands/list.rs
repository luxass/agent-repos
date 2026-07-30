//! `agent-repos list` — the configured entries, as a table or as JSON.

use crate::error::Result;
use crate::manifest::Manifest;
use crate::{git, json, ui};

pub(crate) fn list(json: bool) -> Result<()> {
    let root = git::root()?;
    let manifest = Manifest::load(&root)?;

    if json {
        print!("{}", json::render(&manifest, &root));
        return Ok(());
    }

    if manifest.repos.is_empty() {
        ui::log("no reference repositories configured");
        ui::log("add one with `agent-repos add <url> --tag <version>`");
        return Ok(());
    }

    let width = |pick: fn(&crate::manifest::Repo) -> &str, heading: &str| {
        manifest
            .repos
            .iter()
            .map(|repo| pick(repo).chars().count())
            .chain(std::iter::once(heading.chars().count()))
            .max()
            .unwrap_or(0)
    };

    let name_width = width(|repo| repo.name.as_str(), "NAME");
    let kind_width = width(|repo| repo.kind.as_str(), "KIND");
    let ref_width = width(|repo| repo.git_ref.as_str(), "REF");
    let path_width = width(|repo| repo.path.as_str(), "PATH");

    println!(
        "{:name_width$}  {:kind_width$}  {:ref_width$}  {:path_width$}  STATUS",
        "NAME", "KIND", "REF", "PATH"
    );

    for repo in &manifest.repos {
        let present = root.join(&repo.path).exists();
        let status = match (present, repo.kind.is_pinned()) {
            (false, _) => "missing",
            (true, true) => "present",
            (true, false) => "present (unpinned)",
        };
        println!(
            "{:name_width$}  {:kind_width$}  {:ref_width$}  {:path_width$}  {status}",
            repo.name,
            repo.kind.as_str(),
            repo.git_ref,
            repo.path,
        );
    }

    Ok(())
}
