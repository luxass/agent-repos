//! Wrappers around the system `git`.
//!
//! Spawning `git` rather than linking a git implementation is what keeps this
//! binary small, and it inherits the user's SSH keys, credential helpers,
//! proxies, `GH_TOKEN` and git-lfs without any code here.
//!
//! Queries capture output; anything long-running (clone, fetch) inherits
//! stderr so the user sees git's own progress.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Error, Result};
use crate::manifest::{Kind, Repo};

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

/// Moves a clone onto `reference` without putting it on a local branch.
///
/// Every reference checkout is detached on purpose: there is no branch to be
/// on, and nothing here ever commits.
fn detach(dir: &Path, reference: &str) -> Result<()> {
    run(Some(dir), &["checkout", "--quiet", "--detach", reference])
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

/// Clones an entry at its pinned ref.
///
/// The only way in: a failed clone is cleaned up here, so a partial directory
/// never blocks a retry, and no caller can skip that by reaching for one of the
/// clone routines below directly.
pub(crate) fn clone_pinned(repo: &Repo, dest: &Path) -> Result<()> {
    let result = match repo.kind {
        Kind::Tag | Kind::Branch => clone_ref(&repo.url, &repo.git_ref, dest),
        Kind::Commit => clone_commit(&repo.url, &repo.git_ref, repo.track.as_deref(), dest),
    };

    if result.is_err() && dest.exists() {
        let _ = fs::remove_dir_all(dest);
    }
    result
}

/// Shallow-clones a tag or branch.
fn clone_ref(url: &str, reference: &str, dest: &Path) -> Result<()> {
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
fn clone_commit(url: &str, sha: &str, track: Option<&str>, dest: &Path) -> Result<()> {
    let dest_string = dest.to_string_lossy().into_owned();

    run(None, &["init", "--quiet", &dest_string])?;
    run(Some(dest), &["remote", "add", "origin", url])?;

    if try_run(Some(dest), &["fetch", "--depth", "1", "origin", sha])? {
        return detach(dest, "FETCH_HEAD");
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
            return detach(dest, sha);
        }
    }

    run(Some(dest), &["fetch", "--unshallow", "origin", branch])?;
    detach(dest, sha)
}

/// Every tag on a remote, de-duplicated. Peeled `^{}` entries are filtered out
/// by `--refs`, so each tag appears once.
pub(crate) fn remote_tags(url: &str) -> Result<Vec<String>> {
    let output = capture(None, &["ls-remote", "--tags", "--refs", url])?;

    let mut tags: Vec<String> = output
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter_map(|(_, name)| name.trim().strip_prefix("refs/tags/"))
        .map(str::to_string)
        .collect();

    tags.sort();
    tags.dedup();
    Ok(tags)
}

/// The commit currently checked out in a clone.
pub(crate) fn head_sha(dir: &Path) -> Result<String> {
    Ok(capture(Some(dir), &["rev-parse", "HEAD"])?
        .trim()
        .to_string())
}

/// Resolves a ref inside a clone, or `None` if it is not present locally.
pub(crate) fn local_sha(dir: &Path, reference: &str) -> Option<String> {
    capture(Some(dir), &["rev-parse", "--verify", "--quiet", reference])
        .ok()
        .map(|sha| sha.trim().to_string())
        .filter(|sha| !sha.is_empty())
}

/// Abbreviates a full sha the way git itself displays one, leaving tags and
/// branch names alone.
pub(crate) fn short(git_ref: &str) -> String {
    if git_ref.len() == 40 && git_ref.chars().all(|ch| ch.is_ascii_hexdigit()) {
        git_ref[..7].to_string()
    } else {
        git_ref.to_string()
    }
}

/// Whether a checkout has moved off its pin.
///
/// A branch entry follows a moving target by definition, so it never counts as
/// drifted. A tag counts only once the tag object is present locally; without
/// it there is nothing to compare `head` against.
pub(crate) fn drifted(dir: &Path, repo: &Repo, head: &str) -> bool {
    let expected = match repo.kind {
        Kind::Commit => Some(repo.git_ref.clone()),
        Kind::Tag => local_sha(dir, &format!("refs/tags/{}", repo.git_ref)),
        Kind::Branch => None,
    };
    expected.is_some_and(|sha| sha != head)
}

/// Whether a clone has uncommitted modifications. Reference clones are
/// read-only by contract, so this is worth surfacing.
pub(crate) fn is_dirty(dir: &Path) -> Result<bool> {
    Ok(!capture(Some(dir), &["status", "--porcelain"])?
        .trim()
        .is_empty())
}

/// Whether `dir` looks like a git checkout at all.
pub(crate) fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Moves an existing clone onto an exact commit, fetching it if needed.
pub(crate) fn fetch_commit(dir: &Path, sha: &str) -> Result<()> {
    if local_sha(dir, &format!("{sha}^{{commit}}")).is_none()
        && !try_run(Some(dir), &["fetch", "--depth", "1", "origin", sha])?
    {
        return Err(Error::failure(format!(
            "could not fetch commit {sha}; the remote may have pruned it"
        )));
    }
    detach(dir, sha)
}

/// Moves an existing clone onto a tag.
pub(crate) fn fetch_tag(dir: &Path, tag: &str) -> Result<()> {
    run(
        Some(dir),
        &[
            "fetch",
            "--depth",
            "1",
            "--force",
            "origin",
            &format!("refs/tags/{tag}:refs/tags/{tag}"),
        ],
    )?;
    detach(dir, &format!("refs/tags/{tag}"))
}

/// Fast-forwards a branch checkout to the remote tip.
///
/// A hard reset rather than a pull: reference clones are read-only by
/// contract, so there is nothing to preserve, and this cannot fail the way a
/// non-fast-forward pull does.
pub(crate) fn fetch_and_reset(dir: &Path, branch: &str) -> Result<()> {
    run(
        Some(dir),
        &["fetch", "--depth", "1", "--force", "origin", branch],
    )?;
    detach(dir, "FETCH_HEAD")
}

/// Moves an existing clone onto a pin, the counterpart of [`clone_pinned`].
///
/// Takes the kind and ref loose rather than a [`Repo`], because `update --to`
/// applies a pin the entry does not carry yet.
pub(crate) fn move_to(dir: &Path, kind: Kind, git_ref: &str) -> Result<()> {
    match kind {
        Kind::Tag => fetch_tag(dir, git_ref),
        Kind::Branch => fetch_and_reset(dir, git_ref),
        Kind::Commit => fetch_commit(dir, git_ref),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_found_from_inside_this_repository() {
        let found = root().expect("tests run inside a git repository");
        assert!(found.join(".git").exists());
    }

    #[test]
    fn shortens_only_full_shas() {
        assert_eq!(short("9f3a1c2e5b7d4a6c8e0f2b4d6a8c0e2f4b6d8a0c"), "9f3a1c2");
        assert_eq!(short("v3.12.0"), "v3.12.0");
        assert_eq!(short("main"), "main");
        // 40 characters but not hex: leave it alone.
        assert_eq!(short(&"z".repeat(40)), "z".repeat(40));
    }
}
