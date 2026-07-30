//! Integration tests driving the real binary.
//!
//! No network: fixtures are local repositories created with `git init`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_agent-repos");

/// A scratch directory that cleans itself up.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);

        let path = std::env::temp_dir().join(format!(
            "agent-repos-it-{}-{label}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn git(dir: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git should be on PATH");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// An initialised repository with a committed file, and identity configured so
/// commits work on a machine with no global git config.
fn repo(label: &str) -> Scratch {
    let scratch = Scratch::new(label);
    let dir = &scratch.path;

    git(dir, &["init", "-q", "-b", "main", "."]);
    git(dir, &["config", "user.name", "agent-repos tests"]);
    git(dir, &["config", "user.email", "tests@example.invalid"]);

    scratch
}

struct Run {
    stdout: String,
    stderr: String,
    code: i32,
}

fn run(dir: &Path, args: &[&str]) -> Run {
    let output = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        // Keep assertions free of escape codes regardless of how tests are run.
        .env("NO_COLOR", "1")
        .output()
        .expect("binary should run");

    Run {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        code: output.status.code().unwrap_or(-1),
    }
}

fn manifest(dir: &Path) -> String {
    fs::read_to_string(dir.join(".agent-repos")).expect("manifest should exist")
}

#[test]
fn init_creates_manifest_gitignore_and_clone_dir() {
    let scratch = repo("init");
    let dir = &scratch.path;

    let result = run(dir, &["init"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = manifest(dir);
    assert!(text.contains("version = 1"));
    assert!(text.contains("dir = \"repos\""));
    assert!(text.contains("targets = [\"AGENTS.md\"]"));

    assert!(dir.join("repos").is_dir(), "clone directory should exist");

    let ignore = fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|line| line == "repos/"));
}

#[test]
fn init_leaves_the_manifest_tracked() {
    let scratch = repo("tracked");
    let dir = &scratch.path;

    run(dir, &["init"]);

    // The manifest is the thing teammates restore from, so it must not be
    // swept up by the ignore rule that hides the clone directory.
    let output = Command::new("git")
        .args(["check-ignore", ".agent-repos"])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        ".agent-repos must not be gitignored"
    );
}

#[test]
fn init_adopts_an_existing_instruction_file() {
    let scratch = repo("adopt");
    let dir = &scratch.path;
    fs::write(dir.join("CLAUDE.md"), "# project\n").unwrap();

    run(dir, &["init"]);
    assert!(manifest(dir).contains("targets = [\"CLAUDE.md\"]"));
}

#[test]
fn init_honours_explicit_dir_and_targets() {
    let scratch = repo("explicit");
    let dir = &scratch.path;

    let result = run(
        dir,
        &[
            "init",
            "--dir",
            "vendor",
            "--target",
            "AGENTS.md",
            "--target=CLAUDE.md",
        ],
    );
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = manifest(dir);
    assert!(text.contains("dir = \"vendor\""));
    assert!(text.contains("targets = [\"AGENTS.md\", \"CLAUDE.md\"]"));
    assert!(dir.join("vendor").is_dir());
    assert!(
        fs::read_to_string(dir.join(".gitignore"))
            .unwrap()
            .lines()
            .any(|line| line == "vendor/")
    );
}

#[test]
fn init_with_no_instructions_configures_no_targets() {
    let scratch = repo("no-instructions");
    let dir = &scratch.path;

    run(dir, &["init", "--no-instructions"]);
    assert!(manifest(dir).contains("targets = []"));
}

#[test]
fn init_is_idempotent_and_preserves_entries() {
    let scratch = repo("idempotent");
    let dir = &scratch.path;

    run(dir, &["init"]);
    let first = manifest(dir);

    // A hand-added entry must survive a second init.
    fs::write(
        dir.join(".agent-repos"),
        format!(
            "{first}\n[[repo]]\nname = \"effect\"\nurl = \"https://example.invalid/effect\"\n\
             ref = \"v1.0.0\"\nkind = \"tag\"\npath = \"repos/effect\"\n"
        ),
    )
    .unwrap();

    let result = run(dir, &["init"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let second = manifest(dir);
    assert!(second.contains("name = \"effect\""), "entry was lost");

    // And a third init must not change anything further.
    run(dir, &["init"]);
    assert_eq!(second, manifest(dir), "init should be idempotent");

    let ignore = fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert_eq!(
        ignore.lines().filter(|line| *line == "repos/").count(),
        1,
        "gitignore entry should not be duplicated"
    );
}

#[test]
fn list_reports_presence_per_entry() {
    let scratch = repo("list");
    let dir = &scratch.path;
    run(dir, &["init"]);

    fs::write(
        dir.join(".agent-repos"),
        "version = 1\ndir = \"repos\"\ntargets = []\n\n\
         [[repo]]\nname = \"here\"\nurl = \"https://example.invalid/here\"\n\
         ref = \"v1.0.0\"\nkind = \"tag\"\npath = \"repos/here\"\n\n\
         [[repo]]\nname = \"gone\"\nurl = \"https://example.invalid/gone\"\n\
         ref = \"v2.0.0\"\nkind = \"tag\"\npath = \"repos/gone\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.join("repos").join("here")).unwrap();

    let result = run(dir, &["list"]);
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
    let scratch = repo("json");
    let dir = &scratch.path;
    run(dir, &["init"]);

    fs::write(
        dir.join(".agent-repos"),
        "version = 1\ndir = \"repos\"\ntargets = [\"AGENTS.md\"]\n\n\
         [[repo]]\nname = \"effect\"\nurl = \"https://example.invalid/effect\"\n\
         ref = \"v3.12.0\"\nkind = \"tag\"\npath = \"repos/effect\"\n\
         desc = \"Effect \\\"runtime\\\"\"\n",
    )
    .unwrap();

    let result = run(dir, &["list", "--json"]);
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
    let scratch = repo("malformed");
    let dir = &scratch.path;
    run(dir, &["init"]);

    fs::write(
        dir.join(".agent-repos"),
        "version = 1\n[[repo]]\nnope = \"x\"\n",
    )
    .unwrap();

    let result = run(dir, &["list"]);
    assert_eq!(result.code, 1);
    assert!(result.stderr.contains("line 3"), "{}", result.stderr);
    assert!(result.stderr.contains("unknown key"), "{}", result.stderr);
}

#[test]
fn a_path_escaping_the_clone_directory_is_refused() {
    let scratch = repo("escape");
    let dir = &scratch.path;
    run(dir, &["init"]);

    for path in ["../../evil", "/etc/passwd", "elsewhere/evil"] {
        fs::write(
            dir.join(".agent-repos"),
            format!(
                "version = 1\ndir = \"repos\"\n[[repo]]\nname = \"a\"\nurl = \"u\"\n\
                 ref = \"r\"\nkind = \"tag\"\npath = \"{path}\"\n"
            ),
        )
        .unwrap();

        let result = run(dir, &["list"]);
        assert_eq!(result.code, 1, "{path} should be refused");
    }
}

/// A fixture "remote": two commits, with `v1.0.0` on the first.
struct Upstream {
    scratch: Scratch,
    first: String,
    head: String,
}

fn upstream(label: &str) -> Upstream {
    let scratch = repo(&format!("upstream-{label}"));
    let dir = scratch.path.clone();

    fs::write(dir.join("lib.txt"), "v1\n").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "first"]);
    git(&dir, &["tag", "v1.0.0"]);
    let first = String::from_utf8_lossy(&git(&dir, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();

    fs::write(dir.join("lib.txt"), "v2\n").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "second"]);
    let head = String::from_utf8_lossy(&git(&dir, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string();

    Upstream {
        scratch,
        first,
        head,
    }
}

impl Upstream {
    fn url(&self) -> String {
        self.scratch.path.to_string_lossy().into_owned()
    }
}

#[test]
fn add_with_a_tag_checks_out_that_tag() {
    let up = upstream("tag");
    let scratch = repo("add-tag");
    let dir = &scratch.path;
    run(dir, &["init"]);

    let result = run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    assert_eq!(
        fs::read_to_string(dir.join("repos").join("up").join("lib.txt")).unwrap(),
        "v1\n",
        "the tagged content should be checked out, not the branch head"
    );

    let text = manifest(dir);
    assert!(text.contains("kind = \"tag\""));
    assert!(text.contains("ref = \"v1.0.0\""));
    assert!(!text.contains("track ="), "a tag pin has nothing to track");
}

#[test]
fn add_without_a_ref_pins_the_default_head_commit() {
    let up = upstream("head");
    let scratch = repo("add-head");
    let dir = &scratch.path;
    run(dir, &["init"]);

    let result = run(dir, &["add", &up.url(), "--name", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = manifest(dir);
    assert!(text.contains("kind = \"commit\""));
    assert!(
        text.contains(&format!("ref = \"{}\"", up.head)),
        "should pin the exact head sha"
    );
    assert!(
        text.contains("track = \"main\""),
        "should record where the sha came from"
    );

    // The sha is pinned, so the checkout is the second commit's content.
    assert_eq!(
        fs::read_to_string(dir.join("repos").join("up").join("lib.txt")).unwrap(),
        "v2\n"
    );
    // And it is reported, not silently chosen.
    assert!(result.stderr.contains("head of main"), "{}", result.stderr);
}

#[test]
fn add_with_an_explicit_commit_checks_it_out() {
    let up = upstream("commit");
    let scratch = repo("add-commit");
    let dir = &scratch.path;
    run(dir, &["init"]);

    let result = run(
        dir,
        &["add", &up.url(), "--commit", &up.first, "--name", "up"],
    );
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert_eq!(
        fs::read_to_string(dir.join("repos").join("up").join("lib.txt")).unwrap(),
        "v1\n"
    );
}

#[test]
fn add_derives_a_name_from_the_url() {
    let up = upstream("naming");
    let scratch = repo("add-name");
    let dir = &scratch.path;
    run(dir, &["init"]);

    run(dir, &["add", &up.url(), "--tag", "v1.0.0"]);

    let expected = up.scratch.path.file_name().unwrap().to_string_lossy();
    assert!(manifest(dir).contains(&format!("name = \"{expected}\"")));
}

#[test]
fn add_rejects_duplicates_and_bad_refs_without_writing() {
    let up = upstream("reject");
    let scratch = repo("add-reject");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let before = manifest(dir);

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
        let result = run(dir, &args);
        assert_eq!(result.code, 1, "{args:?} should fail");
        assert!(result.stderr.contains(needle), "{}", result.stderr);
        assert_eq!(manifest(dir), before, "{args:?} must not write");
    }
}

#[test]
fn add_refuses_a_path_outside_the_clone_directory() {
    let up = upstream("escape-add");
    let scratch = repo("add-escape");
    let dir = &scratch.path;
    run(dir, &["init"]);

    for path in ["../../evil", "/tmp/evil", "elsewhere/evil"] {
        let result = run(
            dir,
            &[
                "add",
                &up.url(),
                "--tag",
                "v1.0.0",
                "--name",
                "up",
                "--path",
                path,
            ],
        );
        assert_eq!(result.code, 1, "{path} should be refused");
        assert!(!dir.join("repos").join("up").exists());
    }
}

#[test]
fn add_before_init_says_so() {
    let up = upstream("noinit");
    let scratch = repo("add-noinit");

    let result = run(&scratch.path, &["add", &up.url(), "--tag", "v1.0.0"]);
    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("agent-repos init"),
        "{}",
        result.stderr
    );
}

#[test]
fn restore_reproduces_every_pinned_checkout() {
    let up = upstream("restore");
    let scratch = repo("restore");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(
        dir,
        &["add", &up.url(), "--tag", "v1.0.0", "--name", "tagged"],
    );
    run(dir, &["add", &up.url(), "--name", "pinned"]);

    let expected = manifest(dir);

    // This is the fresh-clone path: repos/ is gitignored, so a teammate has
    // nothing until restore runs.
    fs::remove_dir_all(dir.join("repos")).unwrap();

    let result = run(dir, &["restore"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    assert_eq!(
        fs::read_to_string(dir.join("repos").join("tagged").join("lib.txt")).unwrap(),
        "v1\n"
    );
    assert_eq!(
        fs::read_to_string(dir.join("repos").join("pinned").join("lib.txt")).unwrap(),
        "v2\n"
    );
    assert_eq!(manifest(dir), expected, "restore must not rewrite the pins");

    // Running it again is a no-op.
    let again = run(dir, &["restore"]);
    assert_eq!(again.code, 0);
    assert!(again.stderr.contains("already present"), "{}", again.stderr);
}

#[test]
fn commands_outside_a_git_repository_fail_cleanly() {
    let scratch = Scratch::new("no-git");

    for args in [vec!["init"], vec!["list"]] {
        let result = run(&scratch.path, &args);
        assert_eq!(result.code, 1, "{args:?}");
        assert!(
            result.stderr.contains("not inside a Git repository"),
            "{}",
            result.stderr
        );
    }
}

#[test]
fn usage_errors_exit_two_and_do_not_touch_the_filesystem() {
    let scratch = repo("usage");
    let dir = &scratch.path;

    let result = run(dir, &["init", "--nope"]);
    assert_eq!(result.code, 2);
    assert!(
        !dir.join(".agent-repos").exists(),
        "nothing should be written"
    );
}
