//! Integration tests driving the real binary.
//!
//! No network: fixtures are local repositories created with `git init`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_agent-repos");
const MANIFEST_PATH: &str = ".agent-repos/manifest.toml";
const DEFAULT_REPOS: &str = ".agent-repos/repos";
const LOCK_PATH: &str = ".agent-repos/write.lock";

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
    fs::read_to_string(dir.join(MANIFEST_PATH)).expect("manifest should exist")
}

fn checkout(dir: &Path, name: &str) -> PathBuf {
    dir.join(DEFAULT_REPOS).join(name)
}

#[test]
fn init_creates_manifest_gitignore_and_clone_dir() {
    let scratch = repo("init");
    let dir = &scratch.path;

    let result = run(dir, &["init"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = manifest(dir);
    assert!(text.contains("version = 1"));
    assert!(text.contains(&format!("dir = \"{DEFAULT_REPOS}\"")));
    assert!(text.contains("targets = [\"AGENTS.md\"]"));

    assert!(
        dir.join(DEFAULT_REPOS).is_dir(),
        "clone directory should exist"
    );
    assert!(dir.join(LOCK_PATH).is_file(), "lock file should exist");

    let ignore = fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|line| line == ".agent-repos/repos/"));
    assert!(ignore.lines().any(|line| line == ".agent-repos/write.lock"));
}

#[test]
fn init_leaves_the_manifest_tracked() {
    let scratch = repo("tracked");
    let dir = &scratch.path;

    run(dir, &["init"]);

    // The manifest is the thing teammates restore from, so it must not be
    // swept up by the ignore rule that hides the clone directory.
    let output = Command::new("git")
        .args(["check-ignore", MANIFEST_PATH])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "{MANIFEST_PATH} must not be gitignored"
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
        dir.join(MANIFEST_PATH),
        format!(
            "{first}\n[[repo]]\nname = \"effect\"\nurl = \"https://example.invalid/effect\"\n\
             ref = \"v1.0.0\"\nkind = \"tag\"\npath = \"{DEFAULT_REPOS}/effect\"\n"
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
        ignore
            .lines()
            .filter(|line| *line == ".agent-repos/repos/")
            .count(),
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
        dir.join(MANIFEST_PATH),
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
        dir.join(MANIFEST_PATH),
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
        dir.join(MANIFEST_PATH),
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
            dir.join(MANIFEST_PATH),
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
    // v2.0.0 and v10.0.0 both sit here, so "pick the newest" has to order
    // numerically: as strings, v2.0.0 would win.
    git(&dir, &["tag", "v2.0.0"]);
    git(&dir, &["tag", "v10.0.0"]);
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
        fs::read_to_string(checkout(dir, "up").join("lib.txt")).unwrap(),
        "v1\n",
        "the tagged content should be checked out, not the branch head"
    );

    let text = manifest(dir);
    assert!(text.contains("kind = \"tag\""));
    assert!(text.contains("ref = \"v1.0.0\""));
    assert!(!text.contains("track ="), "a tag pin has nothing to track");
}

#[test]
fn concurrent_adds_preserve_every_manifest_entry() {
    let up = upstream("concurrent-add");
    let scratch = repo("concurrent-add");
    let dir = &scratch.path;
    run(dir, &["init", "--no-instructions"]);

    let children = ["alpha", "beta", "gamma"].map(|name| {
        Command::new(BIN)
            .args([
                "add",
                &up.url(),
                "--tag",
                "v1.0.0",
                "--name",
                name,
                "--no-sync",
            ])
            .current_dir(dir)
            .env("NO_COLOR", "1")
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

    let text = manifest(dir);
    for name in ["alpha", "beta", "gamma"] {
        assert!(
            text.contains(&format!("name = \"{name}\"")),
            "manifest lost `{name}`:\n{text}"
        );
        assert!(checkout(dir, name).is_dir(), "{name} was not cloned");
    }
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
        fs::read_to_string(checkout(dir, "up").join("lib.txt")).unwrap(),
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
        fs::read_to_string(checkout(dir, "up").join("lib.txt")).unwrap(),
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
        assert!(!checkout(dir, "up").exists());
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

    // This is the fresh-clone path: the clone directory is gitignored, so a
    // teammate has nothing until restore runs.
    fs::remove_dir_all(dir.join(DEFAULT_REPOS)).unwrap();

    let result = run(dir, &["restore"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    assert_eq!(
        fs::read_to_string(checkout(dir, "tagged").join("lib.txt")).unwrap(),
        "v1\n"
    );
    assert_eq!(
        fs::read_to_string(checkout(dir, "pinned").join("lib.txt")).unwrap(),
        "v2\n"
    );
    assert_eq!(manifest(dir), expected, "restore must not rewrite the pins");

    // Running it again is a no-op.
    let again = run(dir, &["restore"]);
    assert_eq!(again.code, 0);
    assert!(again.stderr.contains("already present"), "{}", again.stderr);
}

#[test]
fn a_plain_update_never_moves_a_pin() {
    let up = upstream("update-pinned");
    let scratch = repo("update-pinned");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let before = manifest(dir);
    let result = run(dir, &["update", "up"]);

    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert_eq!(manifest(dir), before, "a plain update must not move a pin");
    assert_eq!(
        fs::read_to_string(checkout(dir, "up").join("lib.txt")).unwrap(),
        "v1\n"
    );
    assert!(result.stderr.contains("--latest"), "{}", result.stderr);
}

#[test]
fn update_latest_picks_the_highest_version_not_the_highest_string() {
    let up = upstream("update-latest");
    let scratch = repo("update-latest");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let result = run(dir, &["update", "up", "--latest", "--yes"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    assert!(
        manifest(dir).contains("ref = \"v10.0.0\""),
        "should choose v10.0.0 over v2.0.0"
    );
    assert_eq!(
        fs::read_to_string(checkout(dir, "up").join("lib.txt")).unwrap(),
        "v2\n"
    );

    // Running it again is a no-op.
    let again = run(dir, &["update", "up", "--latest", "--yes"]);
    assert!(again.stderr.contains("already at"), "{}", again.stderr);
}

#[test]
fn update_to_repoints_and_reclassifies() {
    let up = upstream("update-to");
    let scratch = repo("update-to");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v10.0.0", "--name", "up"]);

    // Tag -> tag.
    let result = run(dir, &["update", "up", "--to", "v1.0.0"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert!(manifest(dir).contains("ref = \"v1.0.0\""));
    assert_eq!(
        fs::read_to_string(checkout(dir, "up").join("lib.txt")).unwrap(),
        "v1\n"
    );

    // Tag -> branch, which must also change the recorded kind.
    let result = run(dir, &["update", "up", "--to", "main"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    let text = manifest(dir);
    assert!(text.contains("kind = \"branch\""), "{text}");
    assert!(text.contains("ref = \"main\""), "{text}");

    // Branch -> commit.
    let result = run(dir, &["update", "up", "--to", &up.first]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    let text = manifest(dir);
    assert!(text.contains("kind = \"commit\""), "{text}");
    assert!(text.contains(&format!("ref = \"{}\"", up.first)), "{text}");
}

#[test]
fn update_to_rejects_an_unknown_ref_and_multiple_targets() {
    let up = upstream("update-bad");
    let scratch = repo("update-bad");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "a"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "b"]);

    let before = manifest(dir);

    let result = run(dir, &["update", "a", "--to", "nope"]);
    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("has no tag or branch"),
        "{}",
        result.stderr
    );
    assert_eq!(manifest(dir), before);

    let result = run(dir, &["update", "--all", "--to", "v1.0.0"]);
    assert_eq!(result.code, 2, "--to over many entries is a usage error");
}

#[test]
fn update_restores_a_missing_checkout() {
    let up = upstream("update-missing");
    let scratch = repo("update-missing");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    fs::remove_dir_all(checkout(dir, "up")).unwrap();

    let result = run(dir, &["update", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert_eq!(
        fs::read_to_string(checkout(dir, "up").join("lib.txt")).unwrap(),
        "v1\n"
    );
}

#[test]
fn update_repairs_a_checkout_that_drifted_off_its_pin() {
    let up = upstream("update-drift");
    let scratch = repo("update-drift");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(
        dir,
        &["add", &up.url(), "--commit", &up.first, "--name", "up"],
    );

    // Move the checkout off its pin behind agent-repos' back.
    let clone = checkout(dir, "up");
    git(&clone, &["fetch", "--depth", "1", "origin", &up.head]);
    git(&clone, &["checkout", "--quiet", "--detach", &up.head]);
    assert_eq!(fs::read_to_string(clone.join("lib.txt")).unwrap(), "v2\n");

    let result = run(dir, &["update", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert_eq!(
        fs::read_to_string(clone.join("lib.txt")).unwrap(),
        "v1\n",
        "the pin should win over the working state"
    );
}

#[test]
fn status_reports_drift_and_local_edits() {
    let up = upstream("status");
    let scratch = repo("status");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(
        dir,
        &["add", &up.url(), "--tag", "v1.0.0", "--name", "clean"],
    );
    run(
        dir,
        &["add", &up.url(), "--tag", "v1.0.0", "--name", "dirty"],
    );
    run(
        dir,
        &["add", &up.url(), "--tag", "v1.0.0", "--name", "absent"],
    );

    fs::write(checkout(dir, "dirty").join("lib.txt"), "edited\n").unwrap();
    fs::remove_dir_all(checkout(dir, "absent")).unwrap();

    let result = run(dir, &["status"]);
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

#[test]
fn pin_freezes_a_branch_entry_to_its_current_commit() {
    let up = upstream("pin");
    let scratch = repo("pin");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--branch", "main", "--name", "up"]);

    let result = run(dir, &["pin", "up"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = manifest(dir);
    assert!(text.contains("kind = \"commit\""), "{text}");
    assert!(text.contains(&format!("ref = \"{}\"", up.head)), "{text}");
    assert!(
        text.contains("track = \"main\""),
        "the branch it followed should be remembered: {text}"
    );

    // Pinning again reports no change.
    let again = run(dir, &["pin", "up"]);
    assert_eq!(again.code, 0);
    assert!(again.stderr.contains("already pinned"), "{}", again.stderr);
}

#[test]
fn remove_deletes_the_checkout_and_the_entry() {
    let up = upstream("remove");
    let scratch = repo("remove");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let result = run(dir, &["remove", "up", "--yes"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert!(!checkout(dir, "up").exists());
    assert!(!manifest(dir).contains("name = \"up\""));
}

#[test]
fn remove_keep_files_leaves_the_checkout() {
    let up = upstream("remove-keep");
    let scratch = repo("remove-keep");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let result = run(dir, &["remove", "up", "--keep-files"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert!(checkout(dir, "up").exists());
    assert!(!manifest(dir).contains("name = \"up\""));
}

#[test]
fn remove_without_yes_refuses_when_there_is_no_terminal() {
    let up = upstream("remove-tty");
    let scratch = repo("remove-tty");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let before = manifest(dir);
    let result = run(dir, &["remove", "up"]);

    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(result.stderr.contains("--yes"), "{}", result.stderr);
    assert!(checkout(dir, "up").exists(), "nothing deleted");
    assert_eq!(manifest(dir), before, "manifest untouched");
}

#[test]
fn remove_refuses_a_directory_that_is_not_a_checkout() {
    let up = upstream("remove-notrepo");
    let scratch = repo("remove-notrepo");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    // Something else now occupies the path. Deleting it would be destroying
    // data agent-repos never created.
    fs::remove_dir_all(checkout(dir, "up").join(".git")).unwrap();

    let before = manifest(dir);
    let result = run(dir, &["remove", "up", "--yes"]);

    assert_eq!(result.code, 1);
    assert!(
        result.stderr.contains("not a git checkout"),
        "{}",
        result.stderr
    );
    assert!(checkout(dir, "up").exists());
    assert_eq!(
        manifest(dir),
        before,
        "a refused delete must leave the entry in place"
    );
}

#[test]
fn commands_naming_an_unknown_entry_say_so() {
    let scratch = repo("unknown");
    let dir = &scratch.path;
    run(dir, &["init"]);

    for args in [
        vec!["update", "nope"],
        vec!["remove", "nope", "--yes"],
        vec!["pin", "nope"],
    ] {
        let result = run(dir, &args);
        assert_eq!(result.code, 1, "{args:?}");
        assert!(
            result.stderr.contains("no entry named `nope`"),
            "{args:?}: {}",
            result.stderr
        );
    }
}

fn agents_md(dir: &Path) -> String {
    fs::read_to_string(dir.join("AGENTS.md")).expect("AGENTS.md should exist")
}

#[test]
fn sync_fills_blocks_and_leaves_prose_alone() {
    let up = upstream("sync");
    let scratch = repo("sync");
    let dir = &scratch.path;
    fs::write(dir.join("AGENTS.md"), "# My Service\n\nExisting prose.\n").unwrap();
    run(dir, &["init"]);
    run(
        dir,
        &[
            "add",
            &up.url(),
            "--tag",
            "v1.0.0",
            "--name",
            "effect",
            "--desc",
            "Effect runtime",
        ],
    );

    let text = agents_md(dir);
    assert!(text.starts_with("# My Service\n\nExisting prose.\n"));
    assert!(text.contains("<!-- agent-repos:guidance -->"));
    assert!(text.contains(&format!(
        "| effect | v1.0.0 | {DEFAULT_REPOS}/effect | Effect runtime |"
    )));
}

#[test]
fn sync_is_idempotent_and_check_agrees() {
    let up = upstream("sync-idem");
    let scratch = repo("sync-idem");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    run(dir, &["sync"]);
    let once = agents_md(dir);
    run(dir, &["sync"]);
    assert_eq!(
        once,
        agents_md(dir),
        "sync must be byte-identical on a rerun"
    );

    let check = run(dir, &["sync", "--check"]);
    assert_eq!(check.code, 0, "stderr: {}", check.stderr);
}

#[test]
fn sync_check_exits_one_when_a_block_is_stale() {
    let up = upstream("sync-drift");
    let scratch = repo("sync-drift");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);

    let tampered = agents_md(dir).replace(&format!("{DEFAULT_REPOS}/up"), "somewhere-else");
    fs::write(dir.join("AGENTS.md"), &tampered).unwrap();

    let check = run(dir, &["sync", "--check"]);
    assert_eq!(check.code, 1);
    assert!(check.stderr.contains("out of date"), "{}", check.stderr);
    assert_eq!(agents_md(dir), tampered, "--check must not write anything");

    // And a real sync fixes it.
    assert_eq!(run(dir, &["sync"]).code, 0);
    assert_eq!(run(dir, &["sync", "--check"]).code, 0);
}

#[test]
fn every_block_type_renders() {
    let up = upstream("sync-blocks");
    let scratch = repo("sync-blocks");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(
        dir,
        &[
            "add",
            &up.url(),
            "--tag",
            "v1.0.0",
            "--name",
            "effect",
            "--desc",
            "runtime",
            "--use",
            "API shapes",
        ],
    );

    fs::write(
        dir.join("AGENTS.md"),
        "<!-- agent-repos:repos fields=name,ref,url format=list -->\n\
         <!-- /agent-repos:repos -->\n\n\
         <!-- agent-repos:repo name=effect -->\n\
         <!-- /agent-repos:repo -->\n\n\
         <!-- agent-repos:paths -->\n\
         <!-- /agent-repos:paths -->\n",
    )
    .unwrap();

    let result = run(dir, &["sync"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = agents_md(dir);
    assert!(text.contains("- **effect** — Version: v1.0.0"), "{text}");
    assert!(
        text.contains("**effect** — pinned to `v1.0.0` (tag)"),
        "{text}"
    );
    assert!(text.contains("Consult for: API shapes"), "{text}");
    assert!(
        text.lines()
            .any(|line| line == format!("{DEFAULT_REPOS}/effect")),
        "paths block: {text}"
    );
}

#[test]
fn a_malformed_block_is_reported_and_the_file_is_untouched() {
    let scratch = repo("sync-bad");
    let dir = &scratch.path;
    run(dir, &["init"]);

    for (content, needle) in [
        (
            "<!-- agent-repos:bogus -->\n<!-- /agent-repos:bogus -->\n",
            "unknown block `bogus`",
        ),
        ("prose\n<!-- agent-repos:repos -->\n", "never closed"),
        (
            "<!-- agent-repos:repos fields=nope -->\n<!-- /agent-repos:repos -->\n",
            "unknown field `nope`",
        ),
        (
            "<!-- agent-repos:repo name=absent -->\n<!-- /agent-repos:repo -->\n",
            "no entry named `absent`",
        ),
    ] {
        fs::write(dir.join("AGENTS.md"), content).unwrap();

        let result = run(dir, &["sync"]);
        assert_eq!(result.code, 1, "{content:?}");
        assert!(result.stderr.contains(needle), "{}", result.stderr);
        assert_eq!(agents_md(dir), content, "the file must be left as it was");
    }
}

#[test]
fn sync_follows_the_configured_targets() {
    let scratch = repo("sync-targets");
    let dir = &scratch.path;
    run(
        dir,
        &["init", "--target", "AGENTS.md", "--target=CLAUDE.md"],
    );

    assert_eq!(run(dir, &["sync"]).code, 0);
    assert!(dir.join("AGENTS.md").exists());
    assert!(dir.join("CLAUDE.md").exists());
    assert!(agents_md(dir).contains("Vendored Repositories"));
}

#[test]
fn no_sync_leaves_instruction_files_alone() {
    let up = upstream("no-sync");
    let scratch = repo("no-sync");
    let dir = &scratch.path;
    run(dir, &["init"]);

    let result = run(
        dir,
        &[
            "add",
            &up.url(),
            "--tag",
            "v1.0.0",
            "--name",
            "up",
            "--no-sync",
        ],
    );
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert!(
        !dir.join("AGENTS.md").exists(),
        "--no-sync should not have written AGENTS.md"
    );
}

#[test]
fn removing_an_entry_updates_the_generated_table() {
    let up = upstream("sync-remove");
    let scratch = repo("sync-remove");
    let dir = &scratch.path;
    run(dir, &["init"]);
    run(dir, &["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);
    assert!(agents_md(dir).contains(&format!("{DEFAULT_REPOS}/up")));

    run(dir, &["remove", "up", "--yes"]);
    let text = agents_md(dir);
    assert!(!text.contains(&format!("{DEFAULT_REPOS}/up")), "{text}");
    assert!(
        text.contains("No reference repositories configured"),
        "{text}"
    );
}

#[test]
fn completions_are_emitted_for_every_shell() {
    let scratch = repo("completions");
    let dir = &scratch.path;

    for shell in ["fish", "bash", "zsh"] {
        let result = run(dir, &["completions", shell]);
        assert_eq!(result.code, 0, "{shell}: {}", result.stderr);
        assert!(result.stdout.contains("agent-repos"), "{shell}");
        assert!(result.stdout.lines().count() > 10, "{shell}");
        // Machine-readable output belongs on stdout.
        assert!(result.stderr.is_empty(), "{shell}: {}", result.stderr);
    }

    let result = run(dir, &["completions", "nushell"]);
    assert_eq!(result.code, 2);
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
