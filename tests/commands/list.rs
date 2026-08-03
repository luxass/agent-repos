use std::fs;
use std::process::Command;

use super::support::*;

#[test]
fn list_reports_presence_per_entry() {
    let project = TestGitRepo::new("list");
    let dir = project.path();
    project.run(&["init"]);

    fs::write(
        dir.join(MANIFEST_PATH),
        "version = 1\ndir = \"repos\"\ntargets = []\n\n\
         [[repo]]\nname = \"here\"\nurl = \"https://example.invalid/here\"\n\
         ref = \"v1.0.0\"\nkind = \"tag\"\npath = \"repos/here\"\n\n\
         [[repo]]\nname = \"gone\"\nurl = \"https://example.invalid/gone\"\n\
         ref = \"v2.0.0\"\nkind = \"tag\"\npath = \"repos/gone\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("repos").join("here")).unwrap();

    let result = project.run(&["list"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let here = result
        .stdout
        .lines()
        .find(|line| line.starts_with("here"))
        .expect("here should be listed");
    assert!(here.ends_with("present"), "{here}");

    let gone = result
        .stdout
        .lines()
        .find(|line| line.starts_with("gone"))
        .expect("gone should be listed");
    assert!(gone.ends_with("missing"), "{gone}");
}

#[test]
fn list_json_is_well_formed_and_machine_readable() {
    let project = TestGitRepo::new("json");
    let dir = project.path();
    project.run(&["init"]);

    fs::write(
        dir.join(MANIFEST_PATH),
        "version = 1\ndir = \"repos\"\ntargets = [\"AGENTS.md\"]\n\n\
         [[repo]]\nname = \"effect\"\nurl = \"https://example.invalid/effect\"\n\
         ref = \"v3.12.0\"\nkind = \"tag\"\npath = \"repos/effect\"\n\
         desc = \"Effect \\\"runtime\\\"\"\n",
    )
    .unwrap();

    let result = project.run(&["list", "--json"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    // Diagnostics must stay on stderr so stdout can be piped into a parser.
    assert!(result.stdout.starts_with('{'));
    assert!(result.stdout.contains("\"kind\": \"tag\""));
    assert!(result.stdout.contains(r#""desc": "Effect \"runtime\"""#));

    let parsed = Command::new("python3")
        .arg("-c")
        .arg("import json,sys; json.load(sys.stdin)")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(result.stdout.as_bytes())?;
            child.wait()
        });

    if let Ok(status) = parsed {
        assert!(
            status.success(),
            "output is not valid JSON:\n{}",
            result.stdout
        );
    }
}

#[test]
fn a_malformed_manifest_reports_the_line() {
    let project = TestGitRepo::new("malformed");
    let dir = project.path();
    project.run(&["init"]);

    fs::write(
        dir.join(MANIFEST_PATH),
        "version = 1\n[[repo]]\nnope = \"x\"\n",
    )
    .unwrap();

    let result = project.run(&["list"]);
    assert_eq!(result.code, 1);
    assert!(result.stderr.contains("line 3"), "{}", result.stderr);
    assert!(result.stderr.contains("unknown key"), "{}", result.stderr);
}

#[test]
fn a_path_escaping_the_clone_directory_is_refused() {
    let project = TestGitRepo::new("escape");
    let dir = project.path();
    project.run(&["init"]);

    for path in ["../../evil", "/etc/passwd", "elsewhere/evil"] {
        fs::write(
            dir.join(MANIFEST_PATH),
            format!(
                "version = 1\ndir = \"repos\"\n[[repo]]\nname = \"a\"\nurl = \"u\"\n\
                 ref = \"r\"\nkind = \"tag\"\npath = \"{path}\"\n"
            ),
        )
        .unwrap();

        let result = project.run(&["list"]);
        assert_eq!(result.code, 1, "{path} should be refused");
    }
}
