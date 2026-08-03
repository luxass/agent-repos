use super::support::*;

#[test]
fn pin_freezes_a_branch_entry_to_its_current_commit() {
    let up = Upstream::new("pin");
    let project = TestGitRepo::new("pin");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--branch", "main", "--name", "up"]);

    let result = project.run(&["pin", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = project.manifest();
    assert!(text.contains("kind = \"commit\""), "{text}");
    assert!(text.contains(&format!("ref = \"{}\"", up.head())), "{text}");
    assert!(
        text.contains("track = \"main\""),
        "the branch it followed should be remembered: {text}"
    );

    // Pinning again reports no change.
    let again = project.run(&["pin", "up"]);
    assert_eq!(again.code, 0);
    assert!(again.stderr.contains("already pinned"), "{}", again.stderr);
}
