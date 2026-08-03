use super::support::*;
use std::fs;

#[test]
fn init_creates_manifest_gitignore_and_clone_dir() {
    let project = TestGitRepo::new("init");
    let dir = project.path();

    let result = project.run(&["init"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = project.manifest();
    assert!(text.contains("version = 1"));
    assert!(text.contains(&format!("dir = \"{DEFAULT_REPOS}\"")));
    assert!(text.contains("targets = [\"AGENTS.md\"]"));

    assert!(
        dir.join(DEFAULT_REPOS).is_dir(),
        "clone directory should exist"
    );
    assert!(dir.join(LOCK_PATH).is_file(), "lock file should exist");

    let ignore = fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|line| line == ".agent-repos/repos/"));
    assert!(ignore.lines().any(|line| line == ".agent-repos/write.lock"));
}

#[test]
fn init_leaves_the_manifest_tracked() {
    let project = TestGitRepo::new("tracked");
    project.run(&["init"]);

    // The manifest is the thing teammates restore from, so it must not be
    // swept up by the ignore rule that hides the clone directory.
    assert!(
        !project.git_succeeds(&["check-ignore", MANIFEST_PATH]),
        "{MANIFEST_PATH} must not be gitignored"
    );
}

#[test]
fn init_adopts_an_existing_instruction_file() {
    let project = TestGitRepo::new("adopt");
    let dir = project.path();
    fs::write(dir.join("CLAUDE.md"), "# project\n").unwrap();

    project.run(&["init"]);
    assert!(project.manifest().contains("targets = [\"CLAUDE.md\"]"));
}

#[test]
fn init_honours_explicit_dir_and_targets() {
    let project = TestGitRepo::new("explicit");
    let dir = project.path();

    let result = project.run(&[
        "init",
        "--dir",
        "vendor",
        "--target",
        "AGENTS.md",
        "--target=CLAUDE.md",
    ]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = project.manifest();
    assert!(text.contains("dir = \"vendor\""));
    assert!(text.contains("targets = [\"AGENTS.md\", \"CLAUDE.md\"]"));
    assert!(dir.join("vendor").is_dir());
    assert!(
        fs::read_to_string(dir.join(".gitignore"))
            .unwrap()
            .lines()
            .any(|line| line == "vendor/")
    );
}

#[test]
fn init_with_no_instructions_configures_no_targets() {
    let project = TestGitRepo::new("no-instructions");

    project.run(&["init", "--no-instructions"]);
    assert!(project.manifest().contains("targets = []"));
}

#[test]
fn init_is_idempotent_and_preserves_entries() {
    let project = TestGitRepo::new("idempotent");
    let dir = project.path();

    project.run(&["init"]);
    let first = project.manifest();

    // A hand-added entry must survive a second init.
    fs::write(
        dir.join(MANIFEST_PATH),
        format!(
            "{first}\n[[repo]]\nname = \"effect\"\nurl = \"https://example.invalid/effect\"\n\
             ref = \"v1.0.0\"\nkind = \"tag\"\npath = \"{DEFAULT_REPOS}/effect\"\n"
        ),
    )
    .unwrap();

    let result = project.run(&["init"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let second = project.manifest();
    assert!(second.contains("name = \"effect\""), "entry was lost");

    // And a third init must not change anything further.
    project.run(&["init"]);
    assert_eq!(second, project.manifest(), "init should be idempotent");

    let ignore = fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert_eq!(
        ignore
            .lines()
            .filter(|line| *line == ".agent-repos/repos/")
            .count(),
        1,
        "gitignore entry should not be duplicated"
    );
}
