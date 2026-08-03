//! `agent-repos restore` — clone anything missing at its pinned ref.
//!
//! This is the fresh-checkout path: the clone directory is gitignored, so a
//! teammate has nothing until this runs.

use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::{git, ui};

pub(crate) fn restore() -> Result<()> {
    let root = git::root()?;
    let manifest = Manifest::load(&root)?;

    if manifest.repos.is_empty() {
        ui::log("no reference repositories configured");
        return Ok(());
    }

    let mut restored = 0usize;
    let mut failed = 0usize;

    for repo in &manifest.repos {
        let dest = root.join(&repo.path);
        if dest.exists() {
            continue;
        }

        ui::log(&format!(
            "restoring {} at {} {}",
            repo.name,
            repo.kind.as_str(),
            git::short(&repo.git_ref)
        ));

        match git::clone_pinned(repo, &dest) {
            Ok(()) => restored += 1,
            Err(err) => {
                // One bad entry should not stop the rest from being restored.
                ui::error(&format!("{}: {err}", repo.name));
                failed += 1;
            }
        }
    }

    if restored == 0 && failed == 0 {
        ui::log("everything is already present");
    } else {
        ui::log(&format!("restored {restored} of {}", manifest.repos.len()));
    }

    if failed > 0 {
        return Err(Error::failure(format!(
            "{failed} repositor{} could not be restored",
            if failed == 1 { "y" } else { "ies" }
        )));
    }
    Ok(())
}
