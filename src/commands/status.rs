//! `agent-repos status` — drift, local edits and missing checkouts.

use crate::manifest::{Kind, Manifest};
use crate::ui::Result;
use crate::{git, ui};

pub(crate) fn status() -> Result<()> {
    let root = git::root()?;
    let manifest = Manifest::load(&root)?;

    if manifest.repos.is_empty() {
        ui::log("no reference repositories configured");
        return Ok(());
    }

    let mut issues = 0usize;

    for repo in &manifest.repos {
        let dir = root.join(&repo.path);

        if !dir.exists() {
            println!(
                "{:<20} missing        (run `agent-repos restore`)",
                repo.name
            );
            issues += 1;
            continue;
        }
        if !git::is_repo(&dir) {
            println!("{:<20} not a checkout {}", repo.name, repo.path);
            issues += 1;
            continue;
        }

        let head = git::head_sha(&dir)?;
        let dirty = git::is_dirty(&dir)?;
        let off_pin = git::drifted(&dir, repo, &head);

        let mut notes = Vec::new();
        if off_pin {
            notes.push(format!("drifted from {}", git::short(&repo.git_ref)));
        }
        if dirty {
            notes.push("locally modified".to_string());
        }
        if repo.kind == Kind::Branch {
            notes.push(format!("tracks {} (unpinned)", repo.git_ref));
        }
        if off_pin || dirty {
            issues += 1;
        }

        // A branch entry is unpinned on purpose, so say so without calling it
        // a problem. Only drift and local edits need attention.
        println!(
            "{:<20} {:<14} {} {}",
            repo.name,
            if off_pin || dirty { "attention" } else { "ok" },
            git::short(&head),
            notes.join(", ")
        );
    }

    if issues > 0 {
        ui::log(&format!(
            "{issues} of {} need attention",
            manifest.repos.len()
        ));
    }
    Ok(())
}
