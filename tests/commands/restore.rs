use std::fs;

use super::support::*;

#[test]
fn restore_reproduces_every_pinned_checkout() {
    let up = Upstream::new("restore");
    let project = TestGitRepo::new("restore");
    let dir = project.path();
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "tagged"]);
    project.run(&["add", &up.url(), "--name", "pinned"]);

    let expected = project.manifest();

    // This is the fresh-clone path: the clone directory is gitignored, so a
    // teammate has nothing until restore runs.
    fs::remove_dir_all(dir.join(DEFAULT_REPOS)).unwrap();

    let result = project.run(&["restore"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    assert_eq!(
        fs::read_to_string(project.checkout("tagged").join("lib.txt")).unwrap(),
        "v1\n"
    );
    assert_eq!(
        fs::read_to_string(project.checkout("pinned").join("lib.txt")).unwrap(),
        "v2\n"
    );
    assert_eq!(
        project.manifest(),
        expected,
        "restore must not rewrite the pins"
    );

    // Running it again is a no-op.
    let again = project.run(&["restore"]);
    assert_eq!(again.code, 0);
    assert!(again.stderr.contains("already present"), "{}", again.stderr);
}
