//! `agent-repos init` — prepare a repository for reference repos.

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
