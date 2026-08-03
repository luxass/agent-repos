use std::fs;

use super::support::*;

#[test]
fn remove_deletes_the_checkout_and_the_entry() {
    let up = Upstream::new("remove");
    let project = TestGitRepo::new("remove");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let result = project.run(&["remove", "up", "--yes"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert!(!project.checkout("up").exists());
    assert!(!project.manifest().contains("name = \"up\""));
}

#[test]
fn remove_keep_files_leaves_the_checkout() {
    let up = Upstream::new("remove-keep");
    let project = TestGitRepo::new("remove-keep");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let result = project.run(&["remove", "up", "--keep-files"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert!(project.checkout("up").exists());
    assert!(!project.manifest().contains("name = \"up\""));
}

#[test]
fn remove_without_yes_refuses_when_there_is_no_terminal() {
    let up = Upstream::new("remove-tty");
    let project = TestGitRepo::new("remove-tty");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let before = project.manifest();
    let result = project.run(&["remove", "up"]);

    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(result.stderr.contains("--yes"), "{}", result.stderr);
    assert!(project.checkout("up").exists(), "nothing deleted");
    assert_eq!(project.manifest(), before, "manifest untouched");
}

#[test]
fn remove_refuses_a_directory_that_is_not_a_checkout() {
    let up = Upstream::new("remove-notrepo");
    let project = TestGitRepo::new("remove-notrepo");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    // Something else now occupies the path. Deleting it would be destroying
    // data agent-repos never created.
    fs::remove_dir_all(project.checkout("up").join(".git")).unwrap();

    let before = project.manifest();
    let result = project.run(&["remove", "up", "--yes"]);

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("not a git checkout"),
        "{}",
        result.stderr
    );
    assert!(project.checkout("up").exists());
    assert_eq!(
        project.manifest(),
        before,
        "a refused delete must leave the entry in place"
    );
}

#[cfg(unix)]
#[test]
fn remove_refuses_a_checkout_symlinked_outside_the_repository() {
    use std::os::unix::fs::symlink;

    let upstream = Upstream::new("remove-symlink");
    let project = TestGitRepo::new("remove-symlink");
    project.run(&["init"]);
    project.run(&["add", &upstream.url(), "--tag", "v1.0.0", "--name", "up"]);

    let path = project.checkout("up");
    fs::remove_dir_all(&path).unwrap();
    let upstream_path = std::path::PathBuf::from(upstream.url());
    symlink(&upstream_path, &path).unwrap();

    let before = project.manifest();
    let result = project.run(&["remove", "up", "--yes"]);

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("resolves outside the repository"),
        "{}",
        result.stderr
    );
    assert!(upstream_path.join("lib.txt").exists(), "target untouched");
    assert_eq!(
        project.manifest(),
        before,
        "the entry must remain configured"
    );
}
