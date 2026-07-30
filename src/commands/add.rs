//! `agent-repos add` — add a reference repository pinned to an exact ref.

use crate::error::{Error, Result};
use crate::manifest::{Kind, Manifest, Repo};
use crate::{git, paths, ui};

use super::{auto_sync, checkout, short};

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

/// A parsed `add` invocation.
///
/// A struct rather than seven positional parameters: `name`, `path`, `desc`
/// and `usage` are all `Option<String>`, so transposing two of them would
/// compile cleanly and quietly record the wrong thing.
#[derive(Debug)]
pub(crate) struct AddRequest {
    pub(crate) url: String,
    pub(crate) ref_spec: RefSpec,
    pub(crate) name: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) desc: Option<String>,
    pub(crate) usage: Option<String>,
    pub(crate) no_sync: bool,
}

pub(crate) fn add(request: AddRequest) -> Result<()> {
    let AddRequest {
        url,
        ref_spec,
        name,
        path,
        desc,
        usage,
        no_sync,
    } = request;

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

    if !no_sync {
        auto_sync(&root, &manifest);
    }

    ui::log(&format!(
        "added {name} at {path} ({} {})",
        kind.as_str(),
        short(&git_ref)
    ));
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
