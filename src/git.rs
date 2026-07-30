//! Wrappers around the system `git`.
//!
//! Spawning `git` rather than linking a git implementation is what keeps this
//! binary small, and it inherits the user's SSH keys, credential helpers,
//! proxies, `GH_TOKEN` and git-lfs without any code here.

use std::path::PathBuf;
use std::process::Command;

use crate::error::{Error, Result};

/// The root of the repository containing the working directory.
pub(crate) fn root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|err| Error::failure(format!("could not run git: {err}")))?;

    if !output.status.success() {
        return Err(Error::failure(
            "not inside a Git repository (run `git init` first)",
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Err(Error::failure("git did not report a repository root"));
    }

    Ok(PathBuf::from(path))
}
