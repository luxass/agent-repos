use std::fs;

use super::support::*;

#[test]
fn a_plain_update_never_moves_a_pin() {
    let up = Upstream::new("update-pinned");
    let project = TestGitRepo::new("update-pinned");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let before = project.manifest();
    let result = project.run(&["update", "up"]);

    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert_eq!(
        project.manifest(),
        before,
        "a plain update must not move a pin"
    );
    assert_eq!(
        fs::read_to_string(project.checkout("up").join("lib.txt")).unwrap(),
        "v1\n"
    );
    assert!(result.stderr.contains("--latest"), "{}", result.stderr);
}

#[test]
fn update_latest_picks_the_highest_version_not_the_highest_string() {
    let up = Upstream::new("update-latest");
    let project = TestGitRepo::new("update-latest");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let result = project.run(&["update", "up", "--latest", "--yes"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    assert!(
        project.manifest().contains("ref = \"v10.0.0\""),
        "should choose v10.0.0 over v2.0.0"
    );
    assert_eq!(
        fs::read_to_string(project.checkout("up").join("lib.txt")).unwrap(),
        "v2\n"
    );

    // Running it again is a no-op.
    let again = project.run(&["update", "up", "--latest", "--yes"]);
    assert!(again.stderr.contains("already at"), "{}", again.stderr);
}

#[test]
fn update_to_repoints_and_reclassifies() {
    let up = Upstream::new("update-to");
    let project = TestGitRepo::new("update-to");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v10.0.0", "--name", "up"]);

    // Tag -> tag.
    let result = project.run(&["update", "up", "--to", "v1.0.0"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert!(project.manifest().contains("ref = \"v1.0.0\""));
    assert_eq!(
        fs::read_to_string(project.checkout("up").join("lib.txt")).unwrap(),
        "v1\n"
    );

    // Tag -> branch, which must also change the recorded kind.
    let result = project.run(&["update", "up", "--to", "main"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    let text = project.manifest();
    assert!(text.contains("kind = \"branch\""), "{text}");
    assert!(text.contains("ref = \"main\""), "{text}");

    // Branch -> commit.
    let result = project.run(&["update", "up", "--to", up.first()]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    let text = project.manifest();
    assert!(text.contains("kind = \"commit\""), "{text}");
    assert!(
        text.contains(&format!("ref = \"{}\"", up.first())),
        "{text}"
    );
}

#[test]
fn update_to_rejects_an_unknown_ref_and_multiple_targets() {
    let up = Upstream::new("update-bad");
    let project = TestGitRepo::new("update-bad");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "a"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "b"]);

    let before = project.manifest();

    let result = project.run(&["update", "a", "--to", "nope"]);
    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("has no tag or branch"),
        "{}",
        result.stderr
    );
    assert_eq!(project.manifest(), before);

    let result = project.run(&["update", "--all", "--to", "v1.0.0"]);
    assert_eq!(result.code, 2, "--to over many entries is a usage error");
}

#[test]
fn update_restores_a_missing_checkout() {
    let up = Upstream::new("update-missing");
    let project = TestGitRepo::new("update-missing");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    fs::remove_dir_all(project.checkout("up")).unwrap();

    let result = project.run(&["update", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert_eq!(
        fs::read_to_string(project.checkout("up").join("lib.txt")).unwrap(),
        "v1\n"
    );
}

#[test]
fn update_repairs_a_checkout_that_drifted_off_its_pin() {
    let up = Upstream::new("update-drift");
    let project = TestGitRepo::new("update-drift");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--commit", up.first(), "--name", "up"]);

    // Move the checkout off its pin behind agent-repos' back.
    let clone = project.checkout("up");
    git(&clone, &["fetch", "--depth", "1", "origin", up.head()]);
    git(&clone, &["checkout", "--quiet", "--detach", up.head()]);
    assert_eq!(fs::read_to_string(clone.join("lib.txt")).unwrap(), "v2\n");

    let result = project.run(&["update", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert_eq!(
        fs::read_to_string(clone.join("lib.txt")).unwrap(),
        "v1\n",
        "the pin should win over the working state"
    );
}
