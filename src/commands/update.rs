//! `agent-repos update` — verify, repoint or advance a pin.
//!
//! What a plain update does depends on the pin kind: a tag or commit is
//! verified and repaired but never moved without `--to` or `--latest`, while a
//! branch is a moving target by definition.

use std::path::Path;

use crate::error::{Error, Result};
use crate::manifest::{Kind, Manifest};
use crate::version::newest_tag;
use crate::{git, ui};

use super::{auto_sync, checkout, find_index, short};

/// A parsed `update` invocation. `all`, `latest` and `yes` are all booleans,
/// so they travel together in a named struct rather than by position.
#[derive(Debug)]
pub(crate) struct UpdateRequest {
    pub(crate) names: Vec<String>,
    pub(crate) all: bool,
    pub(crate) to: Option<String>,
    pub(crate) latest: bool,
    pub(crate) yes: bool,
}

pub(crate) fn update(request: UpdateRequest) -> Result<()> {
    let UpdateRequest {
        names,
        all,
        to,
        latest,
        yes,
    } = request;

    let root = git::root()?;
    let mut manifest = Manifest::load(&root)?;

    let selected: Vec<usize> = if all {
        (0..manifest.repos.len()).collect()
    } else {
        names
            .iter()
            .map(|name| find_index(&manifest, name))
            .collect::<Result<_>>()?
    };

    if selected.is_empty() {
        ui::log("no reference repositories configured");
        return Ok(());
    }
    if to.is_some() && selected.len() != 1 {
        return Err(Error::usage(
            "--to changes one entry at a time; name a single repository",
        ));
    }

    let mut changed = false;
    for index in selected {
        if update_one(&root, &mut manifest, index, to.as_deref(), latest, yes)? {
            changed = true;
        }
    }

    if changed {
        manifest.save(&root)?;
        auto_sync(&root, &manifest);
    }
    Ok(())
}

/// Updates a single entry, returning whether the manifest needs saving.
fn update_one(
    root: &Path,
    manifest: &mut Manifest,
    index: usize,
    to: Option<&str>,
    latest: bool,
    yes: bool,
) -> Result<bool> {
    let repo = manifest.repos[index].clone();
    let dir = root.join(&repo.path);

    // Nothing to update against: put the pinned checkout back first.
    if !dir.exists() {
        ui::log(&format!("{}: missing, restoring", repo.name));
        checkout(
            &repo.url,
            &repo.kind,
            &repo.git_ref,
            repo.track.as_deref(),
            &dir,
        )?;
        if to.is_none() && !latest {
            return Ok(false);
        }
    }

    if let Some(target) = to {
        let (kind, git_ref, track) = classify(&repo.url, target)?;
        move_to(&dir, &kind, &git_ref)?;

        let entry = &mut manifest.repos[index];
        ui::log(&format!(
            "{}: {} {} -> {} {}",
            entry.name,
            entry.kind.as_str(),
            short(&entry.git_ref),
            kind.as_str(),
            short(&git_ref)
        ));
        entry.kind = kind;
        entry.git_ref = git_ref;
        entry.track = track
            .or(entry.track.clone())
            .filter(|_| kind == Kind::Commit);
        return Ok(true);
    }

    match repo.kind {
        // A branch is a moving target by definition, so plain update and
        // --latest do the same thing.
        Kind::Branch => {
            git::fetch_and_reset(&dir, &repo.git_ref)?;
            ui::log(&format!(
                "{}: reset to {} at {}",
                repo.name,
                repo.git_ref,
                short(&git::head_sha(&dir)?)
            ));
            Ok(false)
        }

        Kind::Tag if latest => {
            let tags = git::remote_tags(&repo.url)?;
            let Some(newest) = newest_tag(&tags) else {
                return Err(Error::failure(format!(
                    "{}: could not find a version-shaped tag; use --to <ref>",
                    repo.name
                )));
            };
            if *newest == repo.git_ref {
                ui::log(&format!("{}: already at {}", repo.name, repo.git_ref));
                return Ok(false);
            }
            if !ui::confirm(
                &format!("Move {} from {} to {newest}?", repo.name, repo.git_ref),
                yes,
            )? {
                ui::log(&format!("{}: left at {}", repo.name, repo.git_ref));
                return Ok(false);
            }

            git::fetch_tag(&dir, newest)?;
            ui::log(&format!("{}: {} -> {newest}", repo.name, repo.git_ref));
            manifest.repos[index].git_ref = newest.clone();
            Ok(true)
        }

        Kind::Commit if latest => {
            let Some(track) = repo.track.clone() else {
                return Err(Error::failure(format!(
                    "{}: no branch recorded to advance along; use --to <ref>",
                    repo.name
                )));
            };
            let newest = git::remote_sha(&repo.url, &format!("refs/heads/{track}"))?;
            if newest == repo.git_ref {
                ui::log(&format!(
                    "{}: already at {} (head of {track})",
                    repo.name,
                    short(&newest)
                ));
                return Ok(false);
            }
            if !ui::confirm(
                &format!(
                    "Move {} from {} to {} (head of {track})?",
                    repo.name,
                    short(&repo.git_ref),
                    short(&newest)
                ),
                yes,
            )? {
                ui::log(&format!("{}: left at {}", repo.name, short(&repo.git_ref)));
                return Ok(false);
            }

            git::fetch_commit(&dir, &newest)?;
            ui::log(&format!(
                "{}: {} -> {}",
                repo.name,
                short(&repo.git_ref),
                short(&newest)
            ));
            manifest.repos[index].git_ref = newest;
            Ok(true)
        }

        // Pinned, and no instruction to move: verify rather than change.
        Kind::Tag | Kind::Commit => {
            let head = git::head_sha(&dir)?;
            let expected = match repo.kind {
                Kind::Commit => Some(repo.git_ref.clone()),
                _ => git::local_sha(&dir, &format!("refs/tags/{}", repo.git_ref)),
            };

            if expected.is_some_and(|sha| sha != head) {
                ui::log(&format!(
                    "{}: drifted, restoring {} {}",
                    repo.name,
                    repo.kind.as_str(),
                    short(&repo.git_ref)
                ));
                move_to(&dir, &repo.kind, &repo.git_ref)?;
            } else {
                ui::log(&format!(
                    "{}: pinned to {} {} (use --latest to move it)",
                    repo.name,
                    repo.kind.as_str(),
                    short(&repo.git_ref)
                ));
            }
            Ok(false)
        }
    }
}

/// Works out whether a user-supplied ref is a tag, a branch or a commit.
fn classify(url: &str, reference: &str) -> Result<(Kind, String, Option<String>)> {
    if git::remote_sha(url, &format!("refs/tags/{reference}")).is_ok() {
        return Ok((Kind::Tag, reference.to_string(), None));
    }
    if git::remote_sha(url, &format!("refs/heads/{reference}")).is_ok() {
        return Ok((Kind::Branch, reference.to_string(), None));
    }

    let looks_like_a_sha =
        reference.len() >= 7 && reference.chars().all(|ch| ch.is_ascii_hexdigit());
    if looks_like_a_sha {
        return Ok((Kind::Commit, reference.to_string(), None));
    }

    Err(Error::failure(format!(
        "{url} has no tag or branch called `{reference}`, and it is not a commit sha"
    )))
}

fn move_to(dir: &Path, kind: &Kind, git_ref: &str) -> Result<()> {
    match kind {
        Kind::Tag => git::fetch_tag(dir, git_ref),
        Kind::Branch => git::fetch_and_reset(dir, git_ref),
        Kind::Commit => git::fetch_commit(dir, git_ref),
    }
}
