use super::support::*;

#[test]
fn help_and_version_forms_succeed() {
    let project = TestGitRepo::new("help-version");
    for args in [
        vec![],
        vec!["help"],
        vec!["--help"],
        vec!["-h"],
        vec!["version"],
        vec!["--version"],
        vec!["-V"],
    ] {
        let result = project.run(&args);
        assert_eq!(result.code, 0, "{args:?}: {}", result.stderr);
    }
}

#[test]
fn contradictory_and_duplicate_options_are_usage_errors() {
    let project = TestGitRepo::new("contradictory-options");
    for args in [
        vec!["add", "url", "--tag", "v1", "--branch", "main"],
        vec!["add", "url", "--tag", "v1", "--tag", "v2"],
        vec!["update", "--all", "effect"],
        vec!["update", "--all", "--to", "v2", "--latest"],
        vec!["status", "extra"],
        vec!["status", "--unknown"],
    ] {
        let result = project.run(&args);
        assert_eq!(result.code, 2, "{args:?}: {}", result.stderr);
    }
}

#[test]
fn an_option_never_swallows_the_option_after_it() {
    let project = TestGitRepo::new("missing-option-value");
    let result = project.run(&["add", "url", "--desc", "--tag", "v1.0.0"]);

    assert_eq!(result.code, 2);
    assert!(
        result.stderr.contains("--desc requires a value"),
        "{}",
        result.stderr
    );
}

#[test]
fn double_dash_turns_options_into_positionals() {
    let project = TestGitRepo::new("double-dash");
    let result = project.run(&["status", "--", "--unknown"]);

    assert_eq!(result.code, 2);
    assert!(
        result.stderr.contains("positional arguments"),
        "{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("unknown option"),
        "{}",
        result.stderr
    );
}
