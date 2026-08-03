use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[path = "repo.rs"]
mod repo;
#[path = "upstream.rs"]
mod upstream;

pub(crate) use repo::{MANIFEST_PATH, TestGitRepo, git};
pub(crate) use upstream::Upstream;

pub(crate) const DEFAULT_REPOS: &str = ".agent-repos/repos";
pub(crate) const LOCK_PATH: &str = ".agent-repos/write.lock";

impl TestGitRepo {
    pub(crate) fn command(&self, args: &[&str]) -> Command {
        let mut command = repo::agent_repos_command(self.path());
        command.args(args);
        command
    }

    pub(crate) fn git_succeeds(&self, args: &[&str]) -> bool {
        repo::git_output(self.path(), args).status.success()
    }

    pub(crate) fn checkout(&self, name: &str) -> PathBuf {
        self.path().join(DEFAULT_REPOS).join(name)
    }

    pub(crate) fn agents_md(&self) -> String {
        fs::read_to_string(self.path().join("AGENTS.md")).expect("AGENTS.md should exist")
    }
}
