use super::repo::{TestGitRepo, git};
use std::fs;

#[derive(Debug)]
pub(crate) struct Upstream {
    repo: TestGitRepo,
    first: String,
    head: String,
}

impl Upstream {
    pub(crate) fn new(label: &str) -> Self {
        let repo = TestGitRepo::new(&format!("upstream-{label}"));
        let dir = repo.path();

        fs::write(dir.join("lib.txt"), "v1\n").unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-qm", "first"]);
        git(dir, &["tag", "-a", "v1.0.0", "-m", "first release"]);
        let first = String::from_utf8_lossy(&git(dir, &["rev-parse", "HEAD"]).stdout)
            .trim()
            .to_string();

        fs::write(dir.join("lib.txt"), "v2\n").unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-qm", "second"]);
        git(dir, &["tag", "v2.0.0"]);
        git(dir, &["tag", "v10.0.0"]);
        let head = String::from_utf8_lossy(&git(dir, &["rev-parse", "HEAD"]).stdout)
            .trim()
            .to_string();

        Self { repo, first, head }
    }

    pub(crate) fn url(&self) -> String {
        self.repo.path().to_string_lossy().into_owned()
    }

    pub(crate) fn first(&self) -> &str {
        &self.first
    }

    pub(crate) fn head(&self) -> &str {
        &self.head
    }
}
