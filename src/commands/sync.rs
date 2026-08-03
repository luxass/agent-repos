//! `agent-repos sync` — refill the generated blocks in AGENTS.md / CLAUDE.md.
//!
//! The machinery lives in [`crate::instructions`], because `add`, `remove`,
//! `update` and `pin` all refresh the same blocks after changing the manifest.
//! This is only the command around it.

use crate::instructions::{self, SyncMode};
use crate::manifest::Manifest;
use crate::ui::{Error, Result};
use crate::{git, ui};

pub(crate) fn sync(targets: Vec<String>, mode: SyncMode) -> Result<()> {
    let root = git::root()?;
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

    let drifted = instructions::apply(&root, &manifest, &targets, mode)?;

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
