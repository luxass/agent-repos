//! `agent-repos remove` — drop an entry, and optionally its checkout.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::manifest::Manifest;
use crate::{git, paths, sync, ui};

pub(crate) fn remove(name: String, keep_files: bool, yes: bool) -> Result<()> {
    let root = git::root()?;
    let _lock = Manifest::lock(&root)?;
    let mut manifest = Manifest::load(&root)?;
    let index = manifest.position(&name)?;

    let path = manifest.repos[index].path.clone();
    let dir = root.join(&path);
    let delete = !keep_files && dir.exists();

    // Check the deletion is allowed *before* touching the manifest, so a
    // refusal leaves the entry intact rather than dropping it and then failing.
    let resolved = if delete {
        Some(check_removable(&root, &manifest.dir, &path)?)
    } else {
        None
    };

    if delete && !ui::confirm(&format!("Delete {path}?"), yes)? {
        return Err(Error::failure("cancelled"));
    }

    manifest.repos.remove(index);
    manifest.save(&root)?;
    sync::refresh(&root, &manifest);

    if let Some(resolved) = resolved {
        fs::remove_dir_all(&resolved)
            .map_err(|err| Error::failure(format!("could not delete {path}: {err}")))?;
        ui::log(&format!("removed {name} and deleted {path}"));
    } else {
        ui::log(&format!("removed {name} from the manifest"));
        if dir.exists() {
            ui::log(&format!("{path} was left in place"));
        }
    }
    Ok(())
}

/// Verifies a clone directory is safe to delete and returns the resolved path.
///
/// Four independent checks, because this is the one operation that destroys
/// data: the path must be relative and traversal-free, it must sit inside the
/// configured clone directory, it must actually be a git checkout, and after
/// resolving symlinks it must still be under the repository root.
fn check_removable(root: &Path, clone_dir: &str, path: &str) -> Result<PathBuf> {
    paths::validate_relative("path", path)?;
    if !paths::is_inside(clone_dir, path) {
        return Err(Error::failure(format!(
            "refusing to delete {path}: outside {clone_dir}/"
        )));
    }

    let target = root.join(path);
    if !git::is_repo(&target) {
        return Err(Error::failure(format!(
            "refusing to delete {path}: it is not a git checkout"
        )));
    }

    let resolved = target
        .canonicalize()
        .map_err(|err| Error::failure(format!("could not resolve {path}: {err}")))?;
    let root_resolved = root
        .canonicalize()
        .map_err(|err| Error::failure(format!("could not resolve the repository root: {err}")))?;

    if !resolved.starts_with(&root_resolved) {
        return Err(Error::failure(format!(
            "refusing to delete {path}: it resolves outside the repository"
        )));
    }

    Ok(resolved)
}
