# agent-repos

Keep pinned clones of the repositories you depend on inside your own
repository, so coding agents read the real source instead of guessing at an
API.

If your backend is built on Effect, you clone `Effect-TS/effect` at *the exact
version you depend on* into `.agent-repos/repos/effect`. When an agent needs a
signature, a `Layer` pattern, or the way a test is written, it reads the source
you actually run — not a half-remembered version of it.

```sh
agent-repos add github:Effect-TS/effect --tag v3.12.0 \
  --desc "Effect runtime this service is built on" \
  --use "API signatures, Layer/Runtime composition, test style"
```

That records the pin in `.agent-repos/manifest.toml`, clones the tag into
`.agent-repos/repos/effect`, and refreshes the generated block in your
`AGENTS.md`.

## Why

- **Pinned, never guessed.** Every entry records a tag, branch or commit. The
  ref is never inferred from `package.json` or a lockfile. You pin it, or the
  default branch's current head commit gets pinned and printed.
- **Reproducible.** `.agent-repos/manifest.toml` is committed;
  `.agent-repos/repos/` is gitignored. A teammate clones the project, runs
  `agent-repos restore`, and gets every reference repo at the exact same
  commits.
- **Your agent instructions stay current.** `AGENTS.md` and `CLAUDE.md` carry
  comment-delimited blocks that `agent-repos sync` refills from the manifest —
  including *why* each repo is there and what to consult it for.
- **Tiny and static.** Under 600 KB, no dependencies, fully static on Linux.

## Install

Build from source. Requires Rust 1.96.1 (pinned via `rust-toolchain.toml`) and
`git` on `PATH`.

```sh
cargo build --release
cp target/release/agent-repos ~/.local/bin/
```

## Usage

```
agent-repos init    [--dir DIR] [--target FILE]... [--no-instructions]
agent-repos add     <url> [--tag T | --branch B | --commit SHA]
                          [--name N] [--path P] [--desc TEXT] [--use TEXT] [--no-sync]
agent-repos update  [<name>...] [--all] [--to REF] [--latest] [--yes]
agent-repos restore
agent-repos remove  <name> [--keep-files] [--yes]
agent-repos list    [--json]
agent-repos status
agent-repos pin     <name>
agent-repos sync    [--target FILE] [--check]
agent-repos completions <fish|bash|zsh>
```

### Pinning

| Flag | Recorded as | Behaviour on `update` |
| --- | --- | --- |
| `--tag v3.12.0` | `kind = tag` | Pinned. `--latest` lists remote tags and confirms before moving |
| `--commit 9f3a1c2` | `kind = commit` | Pinned. `--latest` advances to the head of the tracked branch |
| `--branch main` | `kind = branch` | Moving target; fetched and reset on every update |
| *(none)* | `kind = commit` | The default branch's current head commit, pinned and printed |

`--to <ref>` repoints any entry to a different tag, branch or commit.

### Repository URLs

Full Git URLs, local paths, and SCP-style SSH remotes work as usual. Common
forges also have short forms:

```sh
agent-repos add github:owner/repo
agent-repos add gitlab:group/repo
agent-repos add gitea:owner/repo
agent-repos add codeberg:owner/repo
```

For another hosted forge, use its domain before the colon:

```sh
agent-repos add git.example.com:owner/repo
```

Short forms expand to HTTPS URLs before being recorded in
`.agent-repos/manifest.toml`.

### Manifest

All tool-owned files live under `.agent-repos/`:

```text
.agent-repos/
├── manifest.toml  # committed
├── write.lock     # ignored; coordinates manifest writes
└── repos/         # ignored; pinned reference checkouts
```

Only `.agent-repos/manifest.toml` is meant to be committed.

```toml
version = 1
dir = ".agent-repos/repos"
targets = ["AGENTS.md", "CLAUDE.md"]

[[repo]]
name = "effect"
url  = "https://github.com/Effect-TS/effect"
ref  = "v3.12.0"
kind = "tag"
path = ".agent-repos/repos/effect"
desc = "Effect runtime and stdlib this service is built on"
use  = "API signatures, Layer/Runtime composition, test style"
```

`desc` and `use` are what make the generated instructions worth reading — they
tell an agent why the repo is there, not just that it exists.

### Generated instruction blocks

Place any subset of these in `AGENTS.md` or `CLAUDE.md`, anywhere in the file.
`agent-repos sync` refills them in place and is idempotent.

```markdown
<!-- agent-repos:guidance -->
<!-- /agent-repos:guidance -->

<!-- agent-repos:repos fields=name,ref,path,desc -->
<!-- /agent-repos:repos -->

<!-- agent-repos:repo name=effect -->
<!-- /agent-repos:repo -->

<!-- agent-repos:paths -->
<!-- /agent-repos:paths -->
```

| Block | Attributes | Renders |
| --- | --- | --- |
| `guidance` | — | Standard prose on treating the configured clone directory as read-only reference |
| `repos` | `fields=`, `format=table\|list` | Every configured repo |
| `repo` | `name=` (required) | One repo's detail |
| `paths` | — | Bare newline-separated paths |

`agent-repos sync --check` exits 1 if anything is out of date, which makes it a
usable pre-commit or CI check.

## Building

```sh
cargo build --release
cargo test --all
cargo clippy --all-targets -- -D warnings
```

Fully static Linux binaries cross-compile from any host — no zig, no Docker, no
`musl-gcc`, because the crate has no C dependencies:

```sh
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

| Target | Size | Linkage |
| --- | --- | --- |
| `aarch64-apple-darwin` | ~421 KB | libSystem dynamic (unavoidable on macOS) |
| `aarch64-unknown-linux-musl` | ~494 KB | fully static |
| `x86_64-unknown-linux-musl` | ~563 KB | static-pie |

Most of that is `std` itself: a hello-world with the same profile already costs
~426 KB on x86_64-musl, because `std` links the backtrace machinery whether or
not the program ever panics. agent-repos adds roughly 137 KB on top of that.
CI fails the build if any binary exceeds 640 KB.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Could not be carried out: bad manifest, git failure, unknown entry. Also `sync --check` when a file is out of date |
| 2 | Usage error: unknown option, missing value, mutually exclusive flags |

Contributor notes and project invariants live in [AGENTS.md](AGENTS.md).

## Prior art

This replaces a bash script of the same name. The rewrite exists to add real
version pinning — the shell version tracked branches only — and to generate the
agent instruction blocks from the manifest instead of writing a fixed block.
