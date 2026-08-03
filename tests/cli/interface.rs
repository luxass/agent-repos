use super::support::*;
use std::fs;

#[test]
fn completions_are_emitted_for_every_shell() {
    let project = TestGitRepo::new("completions");

    for shell in ["fish", "bash", "zsh"] {
        let result = project.run(&["completions", shell]);
        assert_eq!(result.code, 0, "{shell}: {}", result.stderr);
        assert!(result.stdout.contains("agent-repos"), "{shell}");
        assert!(result.stdout.lines().count() > 10, "{shell}");
        // Machine-readable output belongs on stdout.
        assert!(result.stderr.is_empty(), "{shell}: {}", result.stderr);
    }

    let result = project.run(&["completions", "nushell"]);
    assert_eq!(result.code, 2);
}

#[test]
fn commands_outside_a_git_repository_fail_cleanly() {
    let project = TestGitRepo::new("no-git");
    fs::remove_dir_all(project.path().join(".git")).unwrap();

    for args in [vec!["init"], vec!["list"]] {
        let result = project.run(&args);
        assert_eq!(result.code, 1, "{args:?}");
        assert!(
            result.stderr.contains("not inside a Git repository"),
            "{}",
            result.stderr
        );
    }
}

#[test]
fn usage_errors_exit_two_and_do_not_touch_the_filesystem() {
    let project = TestGitRepo::new("usage");
    let dir = project.path();

    let result = project.run(&["init", "--nope"]);
    assert_eq!(result.code, 2);
    assert!(
        !dir.join(".agent-repos").exists(),
        "nothing should be written"
    );
}
