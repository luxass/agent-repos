//! Commands that operate on the manifest and the clone directory.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::manifest::{DEFAULT_DIR, DEFAULT_TARGET, Kind, Manifest, Repo};
use crate::{fsx, git, paths, ui};

/// Files that are treated as agent instructions when none are configured.
const KNOWN_TARGETS: &[&str] = &["AGENTS.md", "CLAUDE.md", "AGENT.md"];

/// Which ref an entry should be pinned to. Never inferred from a package
/// manifest or lockfile: either the user says so, or the default branch's
/// current head commit is pinned and printed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RefSpec {
    Tag(String),
    Branch(String),
    Commit(String),
    DefaultHead,
}

pub(crate) fn init(dir: Option<String>, targets: Vec<String>, no_instructions: bool) -> Result<()> {
    let root = git::root()?;
    let existing = Manifest::path(&root).exists();

    // Re-running init must not discard entries someone already added.
    let mut manifest = if existing {
        Manifest::load(&root)?
    } else {
        Manifest::new(DEFAULT_DIR.to_string(), Vec::new())
    };

    if let Some(dir) = dir {
        paths::validate_relative("dir", &dir)?;
        manifest.dir = dir;
    }

    manifest.targets = if no_instructions {
        Vec::new()
    } else if !targets.is_empty() {
        for target in &targets {
            paths::validate_relative("target", target)?;
        }
        targets
    } else if manifest.targets.is_empty() {
        detect_targets(&root)
    } else {
        manifest.targets
    };

    let clone_dir = root.join(&manifest.dir);
    fs::create_dir_all(&clone_dir).map_err(|err| {
        Error::failure(format!("could not create {}: {err}", clone_dir.display()))
    })?;

    manifest.save(&root)?;

    // The clone directory is local-only; the manifest is what gets committed,
    // because that is what `agent-repos restore` reproduces from.
    let ignored = ensure_gitignore(&root, &format!("{}/", manifest.dir))?;

    ui::log(&format!(
        "{} {}",
        if existing { "updated" } else { "created" },
        Manifest::path(&root).display()
    ));
    if ignored {
        ui::log(&format!("added {}/ to .gitignore", manifest.dir));
    }
    if manifest.targets.is_empty() {
        ui::log("no instruction files configured");
    } else {
        ui::log(&format!(
            "instruction files: {}",
            manifest.targets.join(", ")
        ));
    }
    ui::log("commit .agent-repos so teammates can run `agent-repos restore`");

    Ok(())
}

/// Prefers instruction files that already exist, so `init` adopts whatever the
/// project uses instead of imposing a second one.
fn detect_targets(root: &Path) -> Vec<String> {
    let found: Vec<String> = KNOWN_TARGETS
        .iter()
        .filter(|name| root.join(name).is_file())
        .map(|name| (*name).to_string())
        .collect();

    if found.is_empty() {
        vec![DEFAULT_TARGET.to_string()]
    } else {
        found
    }
}

/// Appends `entry` to `.gitignore` unless an identical line is already there.
/// Returns whether the file was changed.
fn ensure_gitignore(root: &Path, entry: &str) -> Result<bool> {
    let file = root.join(".gitignore");
    let current = fs::read_to_string(&file).unwrap_or_default();

    if current.lines().any(|line| line.trim() == entry) {
        return Ok(false);
    }

    let mut next = current;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(entry);
    next.push('\n');

    fsx::write_atomic(&file, &next)?;
    Ok(true)
}

/// Derives an entry name from a clone URL: the last path segment, with any
/// `.git` suffix and trailing slash removed.
fn name_from_url(url: &str) -> Result<String> {
    let trimmed = url.trim_end_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);

    // Handle scp-style remotes (git@host:owner/repo) as well as URLs.
    let base = trimmed
        .rsplit(['/', ':'])
        .next()
        .filter(|segment| !segment.is_empty());

    base.map(str::to_string).ok_or_else(|| {
        Error::failure(format!(
            "could not work out a name from {url}; pass --name explicitly"
        ))
    })
}

pub(crate) fn add(
    url: String,
    ref_spec: RefSpec,
    name: Option<String>,
    path: Option<String>,
    desc: Option<String>,
    usage: Option<String>,
    _no_sync: bool,
) -> Result<()> {
    let root = git::root()?;
    let mut manifest = Manifest::load(&root)?;

    let name = match name {
        Some(name) => name,
        None => name_from_url(&url)?,
    };
    if name.contains(['/', '\\', '\n', '\t']) {
        return Err(Error::failure(format!(
            "name must not contain path separators or control characters: {name:?}"
        )));
    }

    let path = path.unwrap_or_else(|| format!("{}/{name}", manifest.dir));
    paths::validate_relative("path", &path)?;
    if !paths::is_inside(&manifest.dir, &path) {
        return Err(Error::failure(format!(
            "path {path} is outside the clone directory {}/",
            manifest.dir
        )));
    }

    if let Some(existing) = manifest.repos.iter().find(|repo| repo.name == name) {
        return Err(Error::failure(format!(
            "`{name}` is already configured at {}. Use `agent-repos update {name} --to <ref>` \
             to repoint it.",
            existing.path
        )));
    }
    if let Some(existing) = manifest.repos.iter().find(|repo| repo.path == path) {
        return Err(Error::failure(format!(
            "{path} is already used by `{}`",
            existing.name
        )));
    }

    let dest = root.join(&path);
    if dest.exists() {
        return Err(Error::failure(format!("{path} already exists")));
    }

    // Resolve the pin before cloning, so a typo in a tag fails fast with a
    // message about the tag rather than a wall of git output.
    let (kind, git_ref, track) = match ref_spec {
        RefSpec::Tag(tag) => {
            git::remote_sha(&url, &format!("refs/tags/{tag}"))
                .map_err(|_| Error::failure(format!("{url} has no tag `{tag}`")))?;
            (Kind::Tag, tag, None)
        }
        RefSpec::Branch(branch) => {
            git::remote_sha(&url, &format!("refs/heads/{branch}"))
                .map_err(|_| Error::failure(format!("{url} has no branch `{branch}`")))?;
            (Kind::Branch, branch, None)
        }
        RefSpec::Commit(sha) => (Kind::Commit, sha, None),
        RefSpec::DefaultHead => {
            let head = git::remote_default(&url)?;
            ui::log(&format!(
                "pinning {} at {} (head of {})",
                name,
                short(&head.sha),
                head.branch
            ));
            (Kind::Commit, head.sha, Some(head.branch))
        }
    };

    checkout(&url, &kind, &git_ref, track.as_deref(), &dest)?;

    manifest.repos.push(Repo {
        name: name.clone(),
        url,
        git_ref: git_ref.clone(),
        kind,
        path: path.clone(),
        track,
        desc,
        usage,
        comments: Vec::new(),
    });
    manifest.save(&root)?;

    ui::log(&format!(
        "added {name} at {path} ({} {})",
        kind.as_str(),
        short(&git_ref)
    ));
    Ok(())
}

/// Clones at the pinned ref, cleaning up a partial directory on failure so a
/// retry is not blocked by leftovers.
fn checkout(url: &str, kind: &Kind, git_ref: &str, track: Option<&str>, dest: &Path) -> Result<()> {
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
fn short(git_ref: &str) -> String {
    if git_ref.len() == 40 && git_ref.chars().all(|ch| ch.is_ascii_hexdigit()) {
        git_ref[..7].to_string()
    } else {
        git_ref.to_string()
    }
}

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
            short(&repo.git_ref)
        ));

        match checkout(
            &repo.url,
            &repo.kind,
            &repo.git_ref,
            repo.track.as_deref(),
            &dest,
        ) {
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

/// Orders a tag for "which of these is newest".
///
/// Returns the numeric components plus whether it is a stable release, so that
/// `v1.2.0` sorts above `v1.2.0-rc.1`. Tags that are not version-shaped return
/// `None` and are ignored when picking the newest, rather than being ordered
/// by string comparison, where `v10` would lose to `v9`.
fn version_key(tag: &str) -> Option<(Vec<u64>, bool)> {
    let core = tag
        .strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag);

    let (numbers, suffix) = match core.find(['-', '+']) {
        Some(index) => (&core[..index], &core[index..]),
        None => (core, ""),
    };

    let parts: Option<Vec<u64>> = numbers
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect();

    let parts = parts?;
    if parts.is_empty() {
        return None;
    }
    Some((parts, suffix.is_empty()))
}

/// The highest version-shaped tag, or `None` if none of them look like one.
fn newest_tag(tags: &[String]) -> Option<&String> {
    tags.iter()
        .filter_map(|tag| version_key(tag).map(|key| (key, tag)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, tag)| tag)
}

fn find_index(manifest: &Manifest, name: &str) -> Result<usize> {
    manifest
        .repos
        .iter()
        .position(|repo| repo.name == name)
        .ok_or_else(|| Error::failure(format!("no entry named `{name}` (see `agent-repos list`)")))
}

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

        // What the pin says the checkout should be sitting on.
        let expected = match repo.kind {
            Kind::Commit => Some(repo.git_ref.clone()),
            Kind::Tag => git::local_sha(&dir, &format!("refs/tags/{}", repo.git_ref)),
            Kind::Branch => None,
        };

        let drifted = expected.as_ref().is_some_and(|sha| *sha != head);
        let mut notes = Vec::new();
        if drifted {
            notes.push(format!("drifted from {}", short(&repo.git_ref)));
        }
        if dirty {
            notes.push("locally modified".to_string());
        }
        if repo.kind == Kind::Branch {
            notes.push(format!("tracks {} (unpinned)", repo.git_ref));
        }
        if drifted || dirty {
            issues += 1;
        }

        // A branch entry is unpinned on purpose, so say so without calling it
        // a problem. Only drift and local edits need attention.
        println!(
            "{:<20} {:<14} {} {}",
            repo.name,
            if drifted || dirty { "attention" } else { "ok" },
            short(&head),
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

pub(crate) fn pin(name: String) -> Result<()> {
    let root = git::root()?;
    let mut manifest = Manifest::load(&root)?;
    let index = find_index(&manifest, &name)?;

    let dir = root.join(&manifest.repos[index].path);
    if !git::is_repo(&dir) {
        return Err(Error::failure(format!(
            "`{name}` is not checked out; run `agent-repos restore` first"
        )));
    }

    let head = git::head_sha(&dir)?;
    let repo = &mut manifest.repos[index];

    if repo.kind == Kind::Commit && repo.git_ref == head {
        ui::log(&format!("{name} is already pinned to {}", short(&head)));
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
    ui::log(&format!(
        "pinned {name} to {} (was {})",
        short(&head),
        short(&previous)
    ));
    Ok(())
}

pub(crate) fn remove(name: String, keep_files: bool, yes: bool) -> Result<()> {
    let root = git::root()?;
    let mut manifest = Manifest::load(&root)?;
    let index = find_index(&manifest, &name)?;

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

pub(crate) fn update(
    names: Vec<String>,
    all: bool,
    to: Option<String>,
    latest: bool,
    yes: bool,
) -> Result<()> {
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

pub(crate) fn list(json: bool) -> Result<()> {
    let root = git::root()?;
    let manifest = Manifest::load(&root)?;

    if json {
        print!("{}", render_json(&manifest, &root));
        return Ok(());
    }

    if manifest.repos.is_empty() {
        ui::log("no reference repositories configured");
        ui::log("add one with `agent-repos add <url> --tag <version>`");
        return Ok(());
    }

    let width = |pick: fn(&crate::manifest::Repo) -> &str, heading: &str| {
        manifest
            .repos
            .iter()
            .map(|repo| pick(repo).chars().count())
            .chain(std::iter::once(heading.chars().count()))
            .max()
            .unwrap_or(0)
    };

    let name_width = width(|repo| repo.name.as_str(), "NAME");
    let kind_width = width(|repo| repo.kind.as_str(), "KIND");
    let ref_width = width(|repo| repo.git_ref.as_str(), "REF");
    let path_width = width(|repo| repo.path.as_str(), "PATH");

    println!(
        "{:name_width$}  {:kind_width$}  {:ref_width$}  {:path_width$}  STATUS",
        "NAME", "KIND", "REF", "PATH"
    );

    for repo in &manifest.repos {
        let present = root.join(&repo.path).exists();
        let status = match (present, repo.kind.is_pinned()) {
            (false, _) => "missing",
            (true, true) => "present",
            (true, false) => "present (unpinned)",
        };
        println!(
            "{:name_width$}  {:kind_width$}  {:ref_width$}  {:path_width$}  {status}",
            repo.name,
            repo.kind.as_str(),
            repo.git_ref,
            repo.path,
        );
    }

    Ok(())
}

fn render_json(manifest: &Manifest, root: &Path) -> String {
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"dir\": \"{}\",\n", json_escape(&manifest.dir)));

    out.push_str("  \"targets\": [");
    for (index, target) in manifest.targets.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("\"{}\"", json_escape(target)));
    }
    out.push_str("],\n");

    out.push_str("  \"repos\": [");
    for (index, repo) in manifest.repos.iter().enumerate() {
        out.push_str(if index > 0 { ",\n    {\n" } else { "\n    {\n" });
        out.push_str(&format!(
            "      \"name\": \"{}\",\n",
            json_escape(&repo.name)
        ));
        out.push_str(&format!("      \"url\": \"{}\",\n", json_escape(&repo.url)));
        out.push_str(&format!(
            "      \"ref\": \"{}\",\n",
            json_escape(&repo.git_ref)
        ));
        out.push_str(&format!("      \"kind\": \"{}\",\n", repo.kind.as_str()));
        out.push_str(&format!(
            "      \"path\": \"{}\",\n",
            json_escape(&repo.path)
        ));
        out.push_str(&format!(
            "      \"track\": {},\n",
            json_option(repo.track.as_deref())
        ));
        out.push_str(&format!(
            "      \"desc\": {},\n",
            json_option(repo.desc.as_deref())
        ));
        out.push_str(&format!(
            "      \"use\": {},\n",
            json_option(repo.usage.as_deref())
        ));
        out.push_str(&format!(
            "      \"present\": {}\n",
            root.join(&repo.path).exists()
        ));
        out.push_str("    }");
    }
    out.push_str(if manifest.repos.is_empty() {
        "]\n"
    } else {
        "\n  ]\n"
    });
    out.push_str("}\n");
    out
}

fn json_option(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", json_escape(value)),
        None => "null".to_string(),
    }
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Kind, Repo};

    fn manifest_with(repos: Vec<Repo>) -> Manifest {
        let mut manifest = Manifest::new("repos".to_string(), vec!["AGENTS.md".to_string()]);
        manifest.repos = repos;
        manifest
    }

    fn repo(name: &str, kind: Kind) -> Repo {
        Repo {
            name: name.to_string(),
            url: format!("https://example.com/{name}"),
            git_ref: "v1.0.0".to_string(),
            kind,
            path: format!("repos/{name}"),
            track: None,
            desc: None,
            usage: None,
            comments: Vec::new(),
        }
    }

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn versions_order_numerically_not_lexically() {
        // The whole reason for parsing: "v9" > "v10" as strings.
        let list = tags(&["v9.0.0", "v10.0.0", "v2.0.0"]);
        assert_eq!(newest_tag(&list).map(String::as_str), Some("v10.0.0"));
    }

    #[test]
    fn a_stable_release_beats_its_prereleases() {
        let list = tags(&["v1.2.0-rc.1", "v1.2.0", "v1.2.0-beta"]);
        assert_eq!(newest_tag(&list).map(String::as_str), Some("v1.2.0"));
    }

    #[test]
    fn a_prerelease_still_wins_if_it_is_the_highest_version() {
        let list = tags(&["v1.2.0", "v1.3.0-rc.1"]);
        assert_eq!(newest_tag(&list).map(String::as_str), Some("v1.3.0-rc.1"));
    }

    #[test]
    fn unversioned_tags_are_ignored() {
        let mixed = tags(&["latest", "nightly", "v1.0.0"]);
        assert_eq!(newest_tag(&mixed).map(String::as_str), Some("v1.0.0"));
        let none = tags(&["latest", "stable"]);
        assert_eq!(newest_tag(&none), None);
        assert_eq!(newest_tag(&[]), None);
    }

    #[test]
    fn the_v_prefix_is_optional() {
        assert_eq!(version_key("v1.2.3"), version_key("1.2.3"));
        assert_eq!(
            newest_tag(&tags(&["1.0.0", "2.0.0"])).map(String::as_str),
            Some("2.0.0")
        );
    }

    #[test]
    fn version_key_rejects_non_versions() {
        assert!(version_key("latest").is_none());
        assert!(version_key("v1.x.0").is_none());
        assert!(version_key("").is_none());
        assert!(version_key("release-2024").is_none());
    }

    #[test]
    fn shortens_only_full_shas() {
        assert_eq!(short("9f3a1c2e5b7d4a6c8e0f2b4d6a8c0e2f4b6d8a0c"), "9f3a1c2");
        assert_eq!(short("v3.12.0"), "v3.12.0");
        assert_eq!(short("main"), "main");
        // 40 characters but not hex: leave it alone.
        assert_eq!(short(&"z".repeat(40)), "z".repeat(40));
    }

    #[test]
    fn names_are_derived_from_assorted_url_shapes() {
        for (url, expected) in [
            ("https://github.com/Effect-TS/effect", "effect"),
            ("https://github.com/Effect-TS/effect.git", "effect"),
            ("https://github.com/Effect-TS/effect/", "effect"),
            ("git@github.com:Effect-TS/effect.git", "effect"),
            ("/local/path/to/thing", "thing"),
        ] {
            assert_eq!(name_from_url(url).unwrap(), expected, "{url}");
        }
    }

    #[test]
    fn json_escapes_control_characters() {
        assert_eq!(json_escape("a\"b\\c\nd\te"), "a\\\"b\\\\c\\nd\\te");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
    }

    #[test]
    fn json_renders_null_for_absent_fields() {
        let manifest = manifest_with(vec![repo("effect", Kind::Tag)]);
        let json = render_json(&manifest, Path::new("/nonexistent"));

        assert!(json.contains("\"track\": null"));
        assert!(json.contains("\"desc\": null"));
        assert!(json.contains("\"present\": false"));
        assert!(json.contains("\"kind\": \"tag\""));
    }

    #[test]
    fn json_with_no_repos_is_still_well_formed() {
        let json = render_json(&manifest_with(Vec::new()), Path::new("/nonexistent"));
        assert!(json.contains("\"repos\": []"));
        assert!(json.trim_end().ends_with('}'));
    }

    #[test]
    fn json_separates_multiple_repos() {
        let manifest = manifest_with(vec![repo("a", Kind::Tag), repo("b", Kind::Branch)]);
        let json = render_json(&manifest, Path::new("/nonexistent"));
        assert_eq!(json.matches("\"name\":").count(), 2);
        assert!(json.contains("},\n    {"));
    }
}
