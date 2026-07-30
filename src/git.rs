//! Wrappers around the system `git`.
//!
//! Spawning `git` rather than linking a git implementation is what keeps this
//! binary small, and it inherits the user's SSH keys, credential helpers,
//! proxies, `GH_TOKEN` and git-lfs without any code here.
//!
//! Queries capture output; anything long-running (clone, fetch) inherits
//! stderr so the user sees git's own progress.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Error, Result};

/// Progressively deeper fetches, for servers that refuse to serve an arbitrary
/// commit directly. The last resort is the full history.
const DEEPEN_STEPS: [u32; 2] = [50, 500];

/// Runs git and captures stdout, failing with git's own stderr.
fn capture(dir: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    let output = command
        .args(args)
        .output()
        .map_err(|err| Error::failure(format!("could not run git: {err}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::failure(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs git with stderr inherited, so progress is visible. Returns whether it
/// succeeded rather than erroring, for callers that have a fallback.
fn try_run(dir: Option<&Path>, args: &[&str]) -> Result<bool> {
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    // Every pinned checkout is deliberately detached; the usual lecture about
    // it is noise here.
    command.args(["-c", "advice.detachedHead=false"]);
    let status = command
        .args(args)
        .stdout(Stdio::null())
        .status()
        .map_err(|err| Error::failure(format!("could not run git: {err}")))?;

    Ok(status.success())
}

fn run(dir: Option<&Path>, args: &[&str]) -> Result<()> {
    if try_run(dir, args)? {
        Ok(())
    } else {
        Err(Error::failure(format!("git {} failed", args.join(" "))))
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RemoteHead {
    pub(crate) branch: String,
    pub(crate) sha: String,
}

/// Resolves a remote's default branch and the commit it currently points at.
///
/// This is what backs `add` with no ref flag: the sha is pinned, and the
/// branch is recorded only so `--latest` knows where to look later.
pub(crate) fn remote_default(url: &str) -> Result<RemoteHead> {
    let output = capture(None, &["ls-remote", "--symref", url, "HEAD"])?;

    let mut branch = None;
    let mut sha = None;

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("ref:") {
            let mut parts = rest.split_whitespace();
            if let Some(reference) = parts.next() {
                branch = Some(
                    reference
                        .strip_prefix("refs/heads/")
                        .unwrap_or(reference)
                        .to_string(),
                );
            }
        } else if let Some((candidate, name)) = line.split_once('\t')
            && name.trim() == "HEAD"
        {
            sha = Some(candidate.trim().to_string());
        }
    }

    match (branch, sha) {
        (Some(branch), Some(sha)) => Ok(RemoteHead { branch, sha }),
        _ => Err(Error::failure(format!(
            "could not resolve the default branch of {url}. \
             Check the URL and that you have access."
        ))),
    }
}

/// Resolves a tag or branch name to a commit sha on the remote.
///
/// Annotated tags appear twice, as the tag object and as `<ref>^{}` for the
/// commit it points at. The peeled entry is the one worth recording.
pub(crate) fn remote_sha(url: &str, reference: &str) -> Result<String> {
    let output = capture(None, &["ls-remote", url, reference])?;

    let mut fallback = None;
    for line in output.lines() {
        let Some((sha, name)) = line.split_once('\t') else {
            continue;
        };
        if name.trim().ends_with("^{}") {
            return Ok(sha.trim().to_string());
        }
        fallback.get_or_insert_with(|| sha.trim().to_string());
    }

    fallback.ok_or_else(|| Error::failure(format!("{url} has no ref matching `{reference}`")))
}

/// Shallow-clones a tag or branch.
pub(crate) fn clone_ref(url: &str, reference: &str, dest: &Path) -> Result<()> {
    let dest = dest.to_string_lossy().into_owned();
    run(
        None,
        &[
            "clone",
            "--depth",
            "1",
            "--single-branch",
            "--branch",
            reference,
            url,
            &dest,
        ],
    )
}

/// Checks out an exact commit.
///
/// Tries a direct shallow fetch of the sha first, which GitHub and GitLab both
/// allow. Servers that refuse it fall back to fetching the tracked branch and
/// deepening until the commit is reachable.
pub(crate) fn clone_commit(url: &str, sha: &str, track: Option<&str>, dest: &Path) -> Result<()> {
    let dest_string = dest.to_string_lossy().into_owned();

    run(None, &["init", "--quiet", &dest_string])?;
    run(Some(dest), &["remote", "add", "origin", url])?;

    if try_run(Some(dest), &["fetch", "--depth", "1", "origin", sha])? {
        return run(
            Some(dest),
            &["checkout", "--quiet", "--detach", "FETCH_HEAD"],
        );
    }

    let Some(branch) = track else {
        return Err(Error::failure(format!(
            "{url} would not serve commit {sha} directly, and no branch is \
             recorded to search. Re-add with --branch, or use a tag."
        )));
    };

    for depth in DEEPEN_STEPS {
        let depth = depth.to_string();
        if try_run(Some(dest), &["fetch", "--depth", &depth, "origin", branch])?
            && try_run(
                Some(dest),
                &["cat-file", "-e", &format!("{sha}^{{commit}}")],
            )?
        {
            return run(Some(dest), &["checkout", "--quiet", "--detach", sha]);
        }
    }

    run(Some(dest), &["fetch", "--unshallow", "origin", branch])?;
    run(Some(dest), &["checkout", "--quiet", "--detach", sha])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_found_from_inside_this_repository() {
        let found = root().expect("tests run inside a git repository");
        assert!(found.join(".git").exists());
    }
}
