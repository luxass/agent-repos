//! Commands that operate on the manifest and the clone directory.

use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::manifest::{DEFAULT_DIR, DEFAULT_TARGET, Manifest};
use crate::{fsx, git, paths, ui};

/// Files that are treated as agent instructions when none are configured.
const KNOWN_TARGETS: &[&str] = &["AGENTS.md", "CLAUDE.md", "AGENT.md"];

pub(crate) fn init(dir: Option<String>, targets: Vec<String>, no_instructions: bool) -> Result<()> {
    let root = git::root()?;
    let existing = Manifest::path(&root).exists();

    // Re-running init must not discard entries someone already added.
    let mut manifest = if existing {
        Manifest::load(&root)?
    } else {
        Manifest::new(DEFAULT_DIR.to_string(), Vec::new())
    };

    if let Some(dir) = dir {
        paths::validate_relative("dir", &dir)?;
        manifest.dir = dir;
    }

    manifest.targets = if no_instructions {
        Vec::new()
    } else if !targets.is_empty() {
        for target in &targets {
            paths::validate_relative("target", target)?;
        }
        targets
    } else if manifest.targets.is_empty() {
        detect_targets(&root)
    } else {
        manifest.targets
    };

    let clone_dir = root.join(&manifest.dir);
    fs::create_dir_all(&clone_dir).map_err(|err| {
        Error::failure(format!("could not create {}: {err}", clone_dir.display()))
    })?;

    manifest.save(&root)?;

    // The clone directory is local-only; the manifest is what gets committed,
    // because that is what `agent-repos restore` reproduces from.
    let ignored = ensure_gitignore(&root, &format!("{}/", manifest.dir))?;

    ui::log(&format!(
        "{} {}",
        if existing { "updated" } else { "created" },
        Manifest::path(&root).display()
    ));
    if ignored {
        ui::log(&format!("added {}/ to .gitignore", manifest.dir));
    }
    if manifest.targets.is_empty() {
        ui::log("no instruction files configured");
    } else {
        ui::log(&format!(
            "instruction files: {}",
            manifest.targets.join(", ")
        ));
    }
    ui::log("commit .agent-repos so teammates can run `agent-repos restore`");

    Ok(())
}

/// Prefers instruction files that already exist, so `init` adopts whatever the
/// project uses instead of imposing a second one.
fn detect_targets(root: &Path) -> Vec<String> {
    let found: Vec<String> = KNOWN_TARGETS
        .iter()
        .filter(|name| root.join(name).is_file())
        .map(|name| (*name).to_string())
        .collect();

    if found.is_empty() {
        vec![DEFAULT_TARGET.to_string()]
    } else {
        found
    }
}

/// Appends `entry` to `.gitignore` unless an identical line is already there.
/// Returns whether the file was changed.
fn ensure_gitignore(root: &Path, entry: &str) -> Result<bool> {
    let file = root.join(".gitignore");
    let current = fs::read_to_string(&file).unwrap_or_default();

    if current.lines().any(|line| line.trim() == entry) {
        return Ok(false);
    }

    let mut next = current;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(entry);
    next.push('\n');

    fsx::write_atomic(&file, &next)?;
    Ok(true)
}

pub(crate) fn list(json: bool) -> Result<()> {
    let root = git::root()?;
    let manifest = Manifest::load(&root)?;

    if json {
        print!("{}", render_json(&manifest, &root));
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

fn render_json(manifest: &Manifest, root: &Path) -> String {
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"dir\": \"{}\",\n", json_escape(&manifest.dir)));

    out.push_str("  \"targets\": [");
    for (index, target) in manifest.targets.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("\"{}\"", json_escape(target)));
    }
    out.push_str("],\n");

    out.push_str("  \"repos\": [");
    for (index, repo) in manifest.repos.iter().enumerate() {
        out.push_str(if index > 0 { ",\n    {\n" } else { "\n    {\n" });
        out.push_str(&format!(
            "      \"name\": \"{}\",\n",
            json_escape(&repo.name)
        ));
        out.push_str(&format!("      \"url\": \"{}\",\n", json_escape(&repo.url)));
        out.push_str(&format!(
            "      \"ref\": \"{}\",\n",
            json_escape(&repo.git_ref)
        ));
        out.push_str(&format!("      \"kind\": \"{}\",\n", repo.kind.as_str()));
        out.push_str(&format!(
            "      \"path\": \"{}\",\n",
            json_escape(&repo.path)
        ));
        out.push_str(&format!(
            "      \"track\": {},\n",
            json_option(repo.track.as_deref())
        ));
        out.push_str(&format!(
            "      \"desc\": {},\n",
            json_option(repo.desc.as_deref())
        ));
        out.push_str(&format!(
            "      \"use\": {},\n",
            json_option(repo.usage.as_deref())
        ));
        out.push_str(&format!(
            "      \"present\": {}\n",
            root.join(&repo.path).exists()
        ));
        out.push_str("    }");
    }
    out.push_str(if manifest.repos.is_empty() {
        "]\n"
    } else {
        "\n  ]\n"
    });
    out.push_str("}\n");
    out
}

fn json_option(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", json_escape(value)),
        None => "null".to_string(),
    }
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Kind, Repo};

    fn manifest_with(repos: Vec<Repo>) -> Manifest {
        let mut manifest = Manifest::new("repos".to_string(), vec!["AGENTS.md".to_string()]);
        manifest.repos = repos;
        manifest
    }

    fn repo(name: &str, kind: Kind) -> Repo {
        Repo {
            name: name.to_string(),
            url: format!("https://example.com/{name}"),
            git_ref: "v1.0.0".to_string(),
            kind,
            path: format!("repos/{name}"),
            track: None,
            desc: None,
            usage: None,
            comments: Vec::new(),
        }
    }

    #[test]
    fn json_escapes_control_characters() {
        assert_eq!(json_escape("a\"b\\c\nd\te"), "a\\\"b\\\\c\\nd\\te");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
    }

    #[test]
    fn json_renders_null_for_absent_fields() {
        let manifest = manifest_with(vec![repo("effect", Kind::Tag)]);
        let json = render_json(&manifest, Path::new("/nonexistent"));

        assert!(json.contains("\"track\": null"));
        assert!(json.contains("\"desc\": null"));
        assert!(json.contains("\"present\": false"));
        assert!(json.contains("\"kind\": \"tag\""));
    }

    #[test]
    fn json_with_no_repos_is_still_well_formed() {
        let json = render_json(&manifest_with(Vec::new()), Path::new("/nonexistent"));
        assert!(json.contains("\"repos\": []"));
        assert!(json.trim_end().ends_with('}'));
    }

    #[test]
    fn json_separates_multiple_repos() {
        let manifest = manifest_with(vec![repo("a", Kind::Tag), repo("b", Kind::Branch)]);
        let json = render_json(&manifest, Path::new("/nonexistent"));
        assert_eq!(json.matches("\"name\":").count(), 2);
        assert!(json.contains("},\n    {"));
    }
}
