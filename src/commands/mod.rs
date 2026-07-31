//! One module per command, matching the CLI surface one to one.
//!
//! [`crate::cli`] parses arguments and calls exactly one of these. Anything
//! shared by more than one command lives here; anything used by a single
//! command lives in that command's module.

mod add;
mod init;
mod list;
mod pin;
mod remove;
mod restore;
mod status;
mod update;

pub(crate) use add::{AddRequest, RefSpec, add};
pub(crate) use init::init;
pub(crate) use list::list;
pub(crate) use pin::pin;
pub(crate) use remove::remove;
pub(crate) use restore::restore;
pub(crate) use status::status;
pub(crate) use update::{UpdateRequest, update};

use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::manifest::{Kind, Manifest};
use crate::{git, sync, ui};

/// Refreshes the generated instruction blocks after the manifest changes.
///
/// A problem in a target file — an unknown block, say — must not undo work
/// that already succeeded, so this reports and carries on rather than failing
/// the command. The manifest is already saved; `agent-repos sync` will pick it
/// up once the file is fixed.
pub(super) fn auto_sync(root: &Path, manifest: &Manifest) {
    if manifest.targets.is_empty() {
        return;
    }
    if let Err(err) = sync::apply(root, manifest, &manifest.targets, sync::SyncMode::Quiet) {
        ui::error(&format!("could not refresh instruction files: {err}"));
    }
}

/// Clones at the pinned ref, cleaning up a partial directory on failure so a
/// retry is not blocked by leftovers.
pub(super) fn checkout(
    url: &str,
    kind: &Kind,
    git_ref: &str,
    track: Option<&str>,
    dest: &Path,
) -> Result<()> {
    let result = match kind {
        Kind::Tag | Kind::Branch => git::clone_ref(url, git_ref, dest),
        Kind::Commit => git::clone_commit(url, git_ref, track, dest),
    };

    if result.is_err() && dest.exists() {
        let _ = fs::remove_dir_all(dest);
    }
    result
}

/// Shortens a 40-character sha for display, leaving other refs alone.
pub(super) fn short(git_ref: &str) -> String {
    if git_ref.len() == 40 && git_ref.chars().all(|ch| ch.is_ascii_hexdigit()) {
        git_ref[..7].to_string()
    } else {
        git_ref.to_string()
    }
}

pub(super) fn find_index(manifest: &Manifest, name: &str) -> Result<usize> {
    manifest
        .repos
        .iter()
        .position(|repo| repo.name == name)
        .ok_or_else(|| Error::failure(format!("no entry named `{name}` (see `agent-repos list`)")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_only_full_shas() {
        assert_eq!(short("9f3a1c2e5b7d4a6c8e0f2b4d6a8c0e2f4b6d8a0c"), "9f3a1c2");
        assert_eq!(short("v3.12.0"), "v3.12.0");
        assert_eq!(short("main"), "main");
        // 40 characters but not hex: leave it alone.
        assert_eq!(short(&"z".repeat(40)), "z".repeat(40));
    }
}
