#[path = "support/repo.rs"]
mod support;

use std::fs;

use support::*;

const DEFAULT_REPOS: &str = ".agent-repos/repos";

#[test]
fn malformed_manifests_are_rejected_with_specific_diagnostics() {
    let project = TestGitRepo::new("manifest-errors");
    let dir = project.path();
    assert_eq!(project.run(&["init", "--no-instructions"]).code, 0);

    let entry = |name: &str, path: &str| {
        format!(
            "[[repo]]\nname = \"{name}\"\nurl = \"u\"\nref = \"r\"\n\
             kind = \"tag\"\npath = \"{path}\"\n"
        )
    };
    let cases = [
        (
            "dir = \"repos\"\n".to_string(),
            "missing required key `version`",
        ),
        ("version = 99\n".to_string(), "unsupported version 99"),
        (
            "version = 1\nnope = 2\n".to_string(),
            "unknown key \"nope\"",
        ),
        (
            "version = 1\n[[repo]]\nnope = \"x\"\n".to_string(),
            "unknown key \"nope\"",
        ),
        (
            format!(
                "version = 1\ndir = \"repos\"\n{}{}",
                entry("a", "repos/a"),
                entry("a", "repos/b")
            ),
            "duplicate entry name",
        ),
        (
            format!(
                "version = 1\ndir = \"repos\"\n{}{}",
                entry("a", "repos/a"),
                entry("b", "repos/a")
            ),
            "share the path",
        ),
        (
            "version = 1\ndir = \"repos\"\n[[repo]]\nname = \"a\"\nurl = \"u\"\n\
             ref = \"v1\"\nkind = \"tag\"\npath = \"repos/a\"\ntrack = \"main\"\n"
                .to_string(),
            "only applies to a commit pin",
        ),
        (
            "version = 1\n[table]\n".to_string(),
            "only supported table is [[repo]]",
        ),
        (
            "version = 1\ndir = repos\n".to_string(),
            "strings must be quoted",
        ),
    ];

    for (text, diagnostic) in cases {
        fs::write(dir.join(MANIFEST_PATH), &text).unwrap();
        let result = project.run(&["list"]);
        assert_eq!(result.code, 1, "{text:?}");
        assert!(result.stdout.is_empty(), "{text:?}: {}", result.stdout);
        assert!(
            result.stderr.contains(diagnostic),
            "expected {diagnostic:?} for {text:?}, got {}",
            result.stderr
        );
        assert_eq!(
            fs::read_to_string(dir.join(MANIFEST_PATH)).unwrap(),
            text,
            "a rejected manifest must not be rewritten"
        );
    }
}

#[test]
fn manifest_comments_and_escaped_values_survive_a_rewrite() {
    let project = TestGitRepo::new("manifest-round-trip");
    let dir = project.path();
    assert_eq!(project.run(&["init", "--no-instructions"]).code, 0);

    let original = format!(
        "# project repositories\nversion = 1\ndir = \"{DEFAULT_REPOS}\"\ntargets = []\n\n\
         # runtime note\n[[repo]]\nname = \"up\"\nurl = \"{}\"\nref = \"v1.0.0\"\n\
         kind = \"tag\"\npath = \"{DEFAULT_REPOS}/up\"\ndesc = \"quote \\\" and slash \\\\\"\n",
        "https://example.invalid/up"
    );
    fs::write(dir.join(MANIFEST_PATH), original).unwrap();

    let result = project.run(&["init", "--no-instructions"]);
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.is_empty(), "{}", result.stdout);
    let rewritten = project.manifest();
    assert!(rewritten.starts_with("# project repositories\n"));
    assert!(rewritten.contains("# runtime note\n[[repo]]"));
    assert!(rewritten.contains("desc = \"quote \\\" and slash \\\\\""));
}
