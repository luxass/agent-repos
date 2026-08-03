use super::support::*;
use std::fs;

#[test]
fn add_with_a_tag_checks_out_that_tag() {
    let up = Upstream::new("tag");
    let project = TestGitRepo::new("add-tag");
    project.run(&["init"]);

    let result = project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    assert_eq!(
        fs::read_to_string(project.checkout("up").join("lib.txt")).unwrap(),
        "v1\n",
        "the tagged content should be checked out, not the branch head"
    );

    let text = project.manifest();
    assert!(text.contains("kind = \"tag\""));
    assert!(text.contains("ref = \"v1.0.0\""));
    assert!(!text.contains("track ="), "a tag pin has nothing to track");
}

#[test]
fn concurrent_adds_preserve_every_manifest_entry() {
    let up = Upstream::new("concurrent-add");
    let project = TestGitRepo::new("concurrent-add");
    project.run(&["init", "--no-instructions"]);

    let children = ["alpha", "beta", "gamma"].map(|name| {
        project
            .command(&[
                "add",
                &up.url(),
                "--tag",
                "v1.0.0",
                "--name",
                name,
                "--no-sync",
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("concurrent add should start")
    });

    for child in children {
        let output = child
            .wait_with_output()
            .expect("concurrent add should finish");
        assert!(
            output.status.success(),
            "concurrent add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let text = project.manifest();
    for name in ["alpha", "beta", "gamma"] {
        assert!(
            text.contains(&format!("name = \"{name}\"")),
            "manifest lost `{name}`:\n{text}"
        );
        assert!(project.checkout(name).is_dir(), "{name} was not cloned");
    }
}

#[test]
fn add_without_a_ref_pins_the_default_head_commit() {
    let up = Upstream::new("head");
    let project = TestGitRepo::new("add-head");
    project.run(&["init"]);

    let result = project.run(&["add", &up.url(), "--name", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = project.manifest();
    assert!(text.contains("kind = \"commit\""));
    assert!(
        text.contains(&format!("ref = \"{}\"", up.head())),
        "should pin the exact head sha"
    );
    assert!(
        text.contains("track = \"main\""),
        "should record where the sha came from"
    );

    // The sha is pinned, so the checkout is the second commit's content.
    assert_eq!(
        fs::read_to_string(project.checkout("up").join("lib.txt")).unwrap(),
        "v2\n"
    );
    // And it is reported, not silently chosen.
    assert!(result.stderr.contains("head of main"), "{}", result.stderr);
}

#[test]
fn add_with_an_explicit_commit_checks_it_out() {
    let up = Upstream::new("commit");
    let project = TestGitRepo::new("add-commit");
    project.run(&["init"]);

    let result = project.run(&["add", &up.url(), "--commit", up.first(), "--name", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert_eq!(
        fs::read_to_string(project.checkout("up").join("lib.txt")).unwrap(),
        "v1\n"
    );
}

#[test]
fn add_derives_a_name_from_the_url() {
    let up = Upstream::new("naming");
    let project = TestGitRepo::new("add-name");
    project.run(&["init"]);

    project.run(&["add", &up.url(), "--tag", "v1.0.0"]);

    let url = up.url();
    let expected = std::path::Path::new(&url)
        .file_name()
        .unwrap()
        .to_string_lossy();
    assert!(
        project
            .manifest()
            .contains(&format!("name = \"{expected}\""))
    );
}

#[test]
fn add_rejects_duplicates_and_bad_refs_without_writing() {
    let up = Upstream::new("reject");
    let project = TestGitRepo::new("add-reject");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let before = project.manifest();

    for (args, needle) in [
        (
            vec!["add", &up.url(), "--tag", "v1.0.0", "--name", "up"],
            "already configured",
        ),
        (
            vec!["add", &up.url(), "--tag", "v9.9.9", "--name", "other"],
            "has no tag",
        ),
        (
            vec!["add", &up.url(), "--branch", "nope", "--name", "other"],
            "has no branch",
        ),
    ] {
        let result = project.run(&args);
        assert_eq!(result.code, 1, "{args:?} should fail");
        assert!(result.stderr.contains(needle), "{}", result.stderr);
        assert_eq!(project.manifest(), before, "{args:?} must not write");
    }
}

#[test]
fn add_refuses_a_path_outside_the_clone_directory() {
    let up = Upstream::new("escape-add");
    let project = TestGitRepo::new("add-escape");
    project.run(&["init"]);

    for path in ["../../evil", "/tmp/evil", "elsewhere/evil"] {
        let result = project.run(&[
            "add",
            &up.url(),
            "--tag",
            "v1.0.0",
            "--name",
            "up",
            "--path",
            path,
        ]);
        assert_eq!(result.code, 1, "{path} should be refused");
        assert!(!project.checkout("up").exists());
    }
}

#[test]
fn add_before_init_says_so() {
    let up = Upstream::new("noinit");
    let project = TestGitRepo::new("add-noinit");

    let result = project.run(&["add", &up.url(), "--tag", "v1.0.0"]);
    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("agent-repos init"),
        "{}",
        result.stderr
    );
}

#[test]
fn add_requires_a_name_when_a_url_has_no_repository_path() {
    let project = TestGitRepo::new("add-url-without-repo");
    assert_eq!(project.run(&["init", "--no-instructions"]).code, 0);

    let result = project.run(&["add", "file://invalid/", "--tag", "v1.0.0"]);

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("could not work out a name"),
        "{}",
        result.stderr
    );
}
