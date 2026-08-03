use super::support::*;

#[test]
fn commands_naming_an_unknown_entry_say_so() {
    let project = TestGitRepo::new("unknown");
    project.run(&["init"]);

    for args in [
        vec!["update", "nope"],
        vec!["remove", "nope", "--yes"],
        vec!["pin", "nope"],
    ] {
        let result = project.run(&args);
        assert_eq!(result.code, 1, "{args:?}");
        assert!(
            result.stderr.contains("no entry named `nope`"),
            "{args:?}: {}",
            result.stderr
        );
    }
}
