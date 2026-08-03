use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const MANIFEST_PATH: &str = ".agent-repos/manifest.toml";
const BIN: &str = env!("CARGO_BIN_EXE_agent-repos");

#[derive(Debug)]
pub(crate) struct TestGitRepo {
    path: PathBuf,
}

impl TestGitRepo {
    pub(crate) fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "agent-repos-it-{}-{label}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();

        git(&path, &["init", "-q", "-b", "main", "."]);
        git(&path, &["config", "user.name", "agent-repos tests"]);
        git(&path, &["config", "user.email", "tests@example.invalid"]);
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn run(&self, args: &[&str]) -> CliOutput {
        run(self.path(), args)
    }

    pub(crate) fn manifest(&self) -> String {
        fs::read_to_string(self.path().join(MANIFEST_PATH)).expect("manifest should exist")
    }
}

impl Drop for TestGitRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn isolate_git(command: &mut Command, dir: &Path) {
    command
        .env("GIT_CONFIG_GLOBAL", dir.join("test-global.gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> Output {
    let output = git_output(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub(crate) fn git_output(dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::new("git");
    isolate_git(&mut command, dir);
    command
        .args(["-c", "commit.gpgSign=false", "-c", "tag.gpgSign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git should be on PATH")
}

#[derive(Debug)]
pub(crate) struct CliOutput {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) code: i32,
}

pub(crate) fn agent_repos_command(dir: &Path) -> Command {
    let mut command = Command::new(BIN);
    isolate_git(&mut command, dir);
    command.current_dir(dir).env("NO_COLOR", "1");
    command
}

fn run(dir: &Path, args: &[&str]) -> CliOutput {
    let mut command = agent_repos_command(dir);
    let output = command.args(args).output().expect("binary should run");
    CliOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        code: output.status.code().unwrap_or(-1),
    }
}
