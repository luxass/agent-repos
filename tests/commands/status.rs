use std::fs;

use super::support::*;

#[test]
fn status_reports_drift_and_local_edits() {
    let up = Upstream::new("status");
    let project = TestGitRepo::new("status");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "clean"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "dirty"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "absent"]);

    fs::write(project.checkout("dirty").join("lib.txt"), "edited\n").unwrap();
    fs::remove_dir_all(project.checkout("absent")).unwrap();

    let result = project.run(&["status"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let line = |name: &str| {
        result
            .stdout
            .lines()
            .find(|line| line.starts_with(name))
            .unwrap_or_else(|| panic!("{name} should be listed"))
            .to_string()
    };

    assert!(line("clean").contains("ok"), "{}", line("clean"));
    assert!(
        line("dirty").contains("locally modified"),
        "{}",
        line("dirty")
    );
    assert!(line("absent").contains("missing"), "{}", line("absent"));
}
