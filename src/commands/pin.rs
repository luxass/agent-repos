//! `agent-repos pin` — freeze an entry to the commit currently checked out.

use crate::error::{Error, Result};
use crate::manifest::{Kind, Manifest};
use crate::{git, sync, ui};

pub(crate) fn pin(name: String) -> Result<()> {
    let root = git::root()?;
    let _lock = Manifest::lock(&root)?;
    let mut manifest = Manifest::load(&root)?;
    let index = manifest.position(&name)?;

    let dir = root.join(&manifest.repos[index].path);
    if !git::is_repo(&dir) {
        return Err(Error::failure(format!(
            "`{name}` is not checked out; run `agent-repos restore` first"
        )));
    }

    let head = git::head_sha(&dir)?;
    let repo = &mut manifest.repos[index];

    if repo.kind == Kind::Commit && repo.git_ref == head {
        ui::log(&format!(
            "{name} is already pinned to {}",
            git::short(&head)
        ));
        return Ok(());
    }

    // A branch pin knows which branch it followed; keep that as `track` so a
    // later --latest still has somewhere to look.
    if repo.kind == Kind::Branch {
        repo.track = Some(repo.git_ref.clone());
    }
    let previous = repo.git_ref.clone();
    repo.kind = Kind::Commit;
    repo.git_ref = head.clone();

    manifest.save(&root)?;
    sync::refresh(&root, &manifest);

    ui::log(&format!(
        "pinned {name} to {} (was {})",
        git::short(&head),
        git::short(&previous)
    ));
    Ok(())
}
