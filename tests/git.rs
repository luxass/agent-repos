#[path = "support/git.rs"]
mod support;

use support::*;

fn head(dir: &std::path::Path) -> String {
    String::from_utf8_lossy(&git(dir, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string()
}

fn assert_detached(name: &str, dir: &std::path::Path) {
    assert!(
        !git_succeeds(dir, &["symbolic-ref", "--quiet", "HEAD"]),
        "{name} checkout should have a detached HEAD"
    );
}

#[test]
fn every_pin_kind_creates_an_exact_detached_checkout() {
    let upstream = Upstream::new("detached");
    let project = TestGitRepo::new("detached");
    assert_eq!(project.run(&["init", "--no-instructions"]).code, 0);
    let url = upstream.url();

    for args in [
        vec![
            "add",
            &url,
            "--tag",
            "v1.0.0",
            "--name",
            "tagged",
            "--no-sync",
        ],
        vec![
            "add",
            &url,
            "--branch",
            "main",
            "--name",
            "branched",
            "--no-sync",
        ],
        vec![
            "add",
            &url,
            "--commit",
            upstream.first(),
            "--name",
            "committed",
            "--no-sync",
        ],
    ] {
        let result = project.run(&args);
        assert_eq!(result.code, 0, "{args:?}: {}", result.stderr);
    }

    assert_eq!(head(&project.checkout("tagged")), upstream.first());
    assert_eq!(head(&project.checkout("branched")), upstream.head());
    assert_eq!(head(&project.checkout("committed")), upstream.first());
    for name in ["tagged", "branched", "committed"] {
        assert_detached(name, &project.checkout(name));
    }
}

#[test]
fn a_failed_commit_fetch_removes_the_partial_checkout() {
    let upstream = Upstream::new("failed-fetch");
    let project = TestGitRepo::new("failed-fetch");
    assert_eq!(project.run(&["init", "--no-instructions"]).code, 0);
    let before = project.manifest();

    let result = project.run(&[
        "add",
        &upstream.url(),
        "--commit",
        "0000000000000000000000000000000000000000",
        "--name",
        "broken",
        "--no-sync",
    ]);

    assert_eq!(result.code, 1);
    assert!(!project.checkout("broken").exists());
    assert_eq!(
        project.manifest(),
        before,
        "a failed fetch must not record a pin"
    );
}

#[test]
fn moving_to_an_annotated_tag_stays_clean_and_detached() {
    let upstream = Upstream::new("move-tag");
    let project = TestGitRepo::new("move-tag");
    assert_eq!(project.run(&["init", "--no-instructions"]).code, 0);
    assert_eq!(
        project
            .run(&[
                "add",
                &upstream.url(),
                "--branch",
                "main",
                "--name",
                "up",
                "--no-sync",
            ],)
            .code,
        0
    );

    let moved = project.run(&["update", "up", "--to", "v1.0.0"]);
    assert_eq!(moved.code, 0, "{}", moved.stderr);
    assert_eq!(head(&project.checkout("up")), upstream.first());
    assert_detached("up", &project.checkout("up"));

    let status = project.run(&["status"]);
    assert_eq!(status.code, 0, "{}", status.stderr);
    assert!(status.stdout.contains("ok"), "{}", status.stdout);
    assert!(!status.stdout.contains("drifted"), "{}", status.stdout);
}
