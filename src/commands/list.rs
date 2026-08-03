//! `agent-repos list` — the configured entries, as a table or as JSON.
//!
//! The JSON is hand-rolled rather than pulling in serde, which would cost more
//! than the rest of the binary. Only [`quote`] has anything to think about;
//! the rest of the document is fixed-shape.

use std::path::Path;

use crate::manifest::{Manifest, Repo};
use crate::ui::Result;
use crate::{git, ui};

pub(crate) fn list(json: bool) -> Result<()> {
    let root = git::root()?;
    let manifest = Manifest::load(&root)?;

    if json {
        print!("{}", to_json(&manifest, &root));
        return Ok(());
    }

    if manifest.repos.is_empty() {
        ui::log("no reference repositories configured");
        ui::log("add one with `agent-repos add <url> --tag <version>`");
        return Ok(());
    }

    // Columns are as wide as their widest cell, heading included.
    let width = |pick: fn(&Repo) -> &str, heading: &str| {
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

fn to_json(manifest: &Manifest, root: &Path) -> String {
    let targets: Vec<String> = manifest.targets.iter().map(|t| quote(t)).collect();

    let entries: Vec<String> = manifest
        .repos
        .iter()
        .map(|repo| {
            let fields = [
                ("name", quote(&repo.name)),
                ("url", quote(&repo.url)),
                ("ref", quote(&repo.git_ref)),
                ("kind", quote(repo.kind.as_str())),
                ("path", quote(&repo.path)),
                ("track", nullable(repo.track.as_deref())),
                ("desc", nullable(repo.desc.as_deref())),
                ("use", nullable(repo.usage.as_deref())),
                ("present", root.join(&repo.path).exists().to_string()),
            ];
            let lines: Vec<String> = fields
                .iter()
                .map(|(key, value)| format!("      \"{key}\": {value}"))
                .collect();
            format!("    {{\n{}\n    }}", lines.join(",\n"))
        })
        .collect();

    let repos = if entries.is_empty() {
        String::new()
    } else {
        format!("\n{}\n  ", entries.join(",\n"))
    };

    format!(
        "{{\n  \"dir\": {},\n  \"targets\": [{}],\n  \"repos\": [{repos}]\n}}\n",
        quote(&manifest.dir),
        targets.join(", ")
    )
}

/// A JSON string literal, escapes and all.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');

    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            other => out.push(other),
        }
    }

    out.push('"');
    out
}

fn nullable(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_string(), quote)
}
