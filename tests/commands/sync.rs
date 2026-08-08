use std::fs;

use super::support::*;

#[test]
fn sync_fills_blocks_and_leaves_prose_alone() {
    let up = Upstream::new("sync");
    let project = TestGitRepo::new("sync");
    let dir = project.path();
    fs::write(dir.join("AGENTS.md"), "# My Service\n\nExisting prose.\n").unwrap();
    project.run(&["init"]);
    project.run(&[
        "add",
        &up.url(),
        "--tag",
        "v1.0.0",
        "--name",
        "effect",
        "--desc",
        "Effect runtime",
    ]);

    let text = project.agents_md();
    assert!(text.starts_with("# My Service\n\nExisting prose.\n"));
    assert!(text.contains("<!-- agent-repos:guidance -->"));
    assert!(text.contains(&format!(
        "| effect | v1.0.0 | {DEFAULT_REPOS}/effect | Effect runtime |"
    )));
}

#[test]
fn repository_use_is_named_guidance_and_the_table_remains() {
    let up = Upstream::new("sync-use-guidance");
    let project = TestGitRepo::new("sync-use-guidance");
    project.run(&["init"]);

    let usage = "When looking to use Better Auth, inspect `repos/better-auth/`.";
    let result = project.run(&[
        "add",
        &up.url(),
        "--tag",
        "v1.0.0",
        "--name",
        "better-auth",
        "--use",
        usage,
    ]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = project.agents_md();
    let bullet = format!("- **better-auth** — {usage}");
    assert!(text.lines().any(|line| line == bullet), "{text}");
    assert!(
        text.find(&bullet).unwrap() < text.find("<!-- agent-repos:repos -->").unwrap(),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "| better-auth | v1.0.0 | {DEFAULT_REPOS}/better-auth |  |"
        )),
        "{text}"
    );
}

#[test]
fn multiline_repository_use_stays_in_one_guidance_item() {
    let up = Upstream::new("sync-multiline-use");
    let project = TestGitRepo::new("sync-multiline-use");
    project.run(&["init"]);

    let result = project.run(&[
        "add",
        &up.url(),
        "--tag",
        "v1.0.0",
        "--name",
        "effect",
        "--use",
        "Read `LLMS.md` first.\nThen inspect the source.",
    ]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = project.agents_md();
    assert!(
        text.contains("- **effect** — Read `LLMS.md` first.\n  Then inspect the source."),
        "{text}"
    );
}

#[test]
fn repository_name_cannot_change_guidance_markup() {
    let up = Upstream::new("sync-markdown-name");
    let project = TestGitRepo::new("sync-markdown-name");
    project.run(&["init"]);

    let result = project.run(&[
        "add",
        &up.url(),
        "--tag",
        "v1.0.0",
        "--name",
        "effect_*[docs]~&copy;",
        "--use",
        "Inspect this repository.",
    ]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = project.agents_md();
    assert!(
        text.contains("- **effect\\_\\*\\[docs\\]\\~\\&copy;** — Inspect this repository."),
        "{text}"
    );
}

#[test]
fn guidance_uses_meaningful_values_in_manifest_order() {
    let project = TestGitRepo::new("sync-use-order");
    let dir = project.path();
    project.run(&["init"]);

    fs::write(
        dir.join(MANIFEST_PATH),
        format!(
            "version = 1\ndir = \"{DEFAULT_REPOS}\"\ntargets = [\"AGENTS.md\"]\n\n\
             [[repo]]\nname = \"first\"\nurl = \"https://example.invalid/first\"\n\
             ref = \"v1\"\nkind = \"tag\"\npath = \"{DEFAULT_REPOS}/first\"\n\
             use = \"First guidance.\"\n\n\
             [[repo]]\nname = \"missing\"\nurl = \"https://example.invalid/missing\"\n\
             ref = \"v1\"\nkind = \"tag\"\npath = \"{DEFAULT_REPOS}/missing\"\n\n\
             [[repo]]\nname = \"blank\"\nurl = \"https://example.invalid/blank\"\n\
             ref = \"v1\"\nkind = \"tag\"\npath = \"{DEFAULT_REPOS}/blank\"\n\
             use = \"   \"\n\n\
             [[repo]]\nname = \"last\"\nurl = \"https://example.invalid/last\"\n\
             ref = \"v1\"\nkind = \"tag\"\npath = \"{DEFAULT_REPOS}/last\"\n\
             use = \"Last guidance.\"\n"
        ),
    )
    .unwrap();

    let result = project.run(&["sync"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = project.agents_md();
    let first = text.find("- **first** — First guidance.").unwrap();
    let last = text.find("- **last** — Last guidance.").unwrap();
    assert!(first < last, "{text}");
    assert!(!text.contains("- **missing** —"), "{text}");
    assert!(!text.contains("- **blank** —"), "{text}");
    assert!(
        text.contains(&format!("| missing | v1 | {DEFAULT_REPOS}/missing |  |")),
        "{text}"
    );
    assert!(
        text.contains(&format!("| blank | v1 | {DEFAULT_REPOS}/blank |  |")),
        "{text}"
    );
}

#[test]
fn sync_is_idempotent_and_check_agrees() {
    let up = Upstream::new("sync-idem");
    let project = TestGitRepo::new("sync-idem");
    project.run(&["init"]);
    project.run(&[
        "add",
        &up.url(),
        "--tag",
        "v1.0.0",
        "--name",
        "up",
        "--use",
        "Inspect this repository.",
    ]);

    project.run(&["sync"]);
    let once = project.agents_md();
    project.run(&["sync"]);
    assert_eq!(
        once,
        project.agents_md(),
        "sync must be byte-identical on a rerun"
    );

    let check = project.run(&["sync", "--check"]);
    assert_eq!(check.code, 0, "stderr: {}", check.stderr);
}

#[test]
fn sync_check_exits_one_when_a_block_is_stale() {
    let up = Upstream::new("sync-drift");
    let project = TestGitRepo::new("sync-drift");
    let dir = project.path();
    project.run(&["init"]);
    project.run(&[
        "add",
        &up.url(),
        "--tag",
        "v1.0.0",
        "--name",
        "up",
        "--use",
        "Inspect this repository.",
    ]);

    let tampered = project
        .agents_md()
        .replace("Inspect this repository.", "Tampered guidance.");
    fs::write(dir.join("AGENTS.md"), &tampered).unwrap();

    let check = project.run(&["sync", "--check"]);
    assert_eq!(check.code, 1);
    assert!(check.stderr.contains("out of date"), "{}", check.stderr);
    assert_eq!(
        project.agents_md(),
        tampered,
        "--check must not write anything"
    );

    // And a real sync fixes it.
    assert_eq!(project.run(&["sync"]).code, 0);
    assert_eq!(project.run(&["sync", "--check"]).code, 0);
}

#[test]
fn every_block_type_renders() {
    let up = Upstream::new("sync-blocks");
    let project = TestGitRepo::new("sync-blocks");
    let dir = project.path();
    project.run(&["init"]);
    project.run(&[
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
    ]);

    fs::write(
        dir.join("AGENTS.md"),
        "<!-- agent-repos:repos fields=name,ref,url,use format=list -->\n\
         <!-- /agent-repos:repos -->\n\n\
         <!-- agent-repos:repo name=effect -->\n\
         <!-- /agent-repos:repo -->\n\n\
         <!-- agent-repos:paths -->\n\
         <!-- /agent-repos:paths -->\n",
    )
    .unwrap();

    let result = project.run(&["sync"]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);

    let text = project.agents_md();
    assert!(
        text.lines().any(|line| {
            line == format!(
                "- **effect** — Version: v1.0.0, URL: {}, Consult for: API shapes",
                up.url()
            )
        }),
        "{text}"
    );
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
    let project = TestGitRepo::new("sync-bad");
    let dir = project.path();
    project.run(&["init"]);

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

        let result = project.run(&["sync"]);
        assert_eq!(result.code, 1, "{content:?}");
        assert!(result.stderr.contains(needle), "{}", result.stderr);
        assert_eq!(
            project.agents_md(),
            content,
            "the file must be left as it was"
        );
    }
}

#[test]
fn sync_follows_the_configured_targets() {
    let project = TestGitRepo::new("sync-targets");
    let dir = project.path();
    project.run(&["init", "--target", "AGENTS.md", "--target=CLAUDE.md"]);

    assert_eq!(project.run(&["sync"]).code, 0);
    assert!(dir.join("AGENTS.md").exists());
    assert!(dir.join("CLAUDE.md").exists());
    assert!(project.agents_md().contains("Vendored Repositories"));
}

#[test]
fn no_sync_leaves_instruction_files_alone() {
    let up = Upstream::new("no-sync");
    let project = TestGitRepo::new("no-sync");
    let dir = project.path();
    project.run(&["init"]);

    let result = project.run(&[
        "add",
        &up.url(),
        "--tag",
        "v1.0.0",
        "--name",
        "up",
        "--no-sync",
    ]);
    assert_eq!(result.code, 0, "stderr: {}", result.stderr);
    assert!(
        !dir.join("AGENTS.md").exists(),
        "--no-sync should not have written AGENTS.md"
    );
}

#[test]
fn removing_an_entry_updates_the_generated_table() {
    let up = Upstream::new("sync-remove");
    let project = TestGitRepo::new("sync-remove");
    project.run(&["init"]);
    project.run(&["add", &up.url(), "--tag", "v1.0.0", "--name", "up"]);
    assert!(project.agents_md().contains(&format!("{DEFAULT_REPOS}/up")));

    project.run(&["remove", "up", "--yes"]);
    let text = project.agents_md();
    assert!(!text.contains(&format!("{DEFAULT_REPOS}/up")), "{text}");
    assert!(
        text.contains("No reference repositories configured"),
        "{text}"
    );
}
