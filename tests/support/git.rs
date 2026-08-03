use std::path::{Path, PathBuf};

#[path = "repo.rs"]
mod repo;
#[path = "upstream.rs"]
mod upstream;

pub(crate) use repo::{TestGitRepo, git};
pub(crate) use upstream::Upstream;

impl TestGitRepo {
    pub(crate) fn checkout(&self, name: &str) -> PathBuf {
        self.path().join(".agent-repos/repos").join(name)
    }
}

pub(crate) fn git_succeeds(dir: &Path, args: &[&str]) -> bool {
    repo::git_output(dir, args).status.success()
}
