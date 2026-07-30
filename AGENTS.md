# AGENT-REPOS

A CLI that maintains pinned clones of external repositories under `repos/`, so
coding agents read a dependency's real source instead of guessing at its API.

Written in Rust with **zero dependencies**. The whole point of the project is a
tiny, statically-linked binary that works everywhere.

## HARD INVARIANTS

Do not break these without the maintainer explicitly agreeing. Each one is
enforced somewhere, and each exists for a stated reason.

| Invariant | Enforced by | Why |
| --- | --- | --- |
| No dependencies. `std` only. | Empty `[dependencies]`, CI size guard | `clap` alone costs more than the rest of the binary |
| Binary stays under 512 KB | Size guard in `.github/workflows/ci.yml` | The reason the project exists |
| No `unsafe` | `unsafe_code = "forbid"` in `Cargo.toml` | Nothing here needs it |
| Shell out to `git`, never link a git library | Code review | `gix` would add 5-8 MB and mean reimplementing credential handling. Spawning `git` inherits SSH keys, credential helpers, proxies, `GH_TOKEN` and git-lfs for free |
| No C dependencies and no `build.rs` | Code review | This is what lets musl targets cross-compile from any host with no zig, Docker or musl-gcc |
| Every version pinned exactly | `rust-toolchain.toml`, `Cargo.lock`, SHA-pinned actions | See PINNING below |
| A ref is never inferred from `package.json` or a lockfile | Code review | The user pins deliberately, or the default branch's current head commit is pinned |

If a change pushes the binary over the limit, do not raise the limit. Work out
what pulled the weight in and remove it.

## STRUCTURE

```text
agent-repos/
|-- src/
|   |-- main.rs           # Command dispatch, option structs, exit codes
|   |-- args.rs           # Hand-rolled argument parser
|   |-- error.rs          # Error type and ExitCode
|   |-- ui.rs             # stderr logging, colour detection
|   |-- manifest.rs       # (planned) .agent-repos TOML subset parse/write
|   |-- git.rs            # (planned) std::process::Command wrappers
|   |-- repo.rs           # (planned) add/update/remove/restore/pin
|   |-- sync.rs           # (planned) AGENTS.md block scan and rewrite
|   `-- paths.rs          # (planned) relative-path validation, containment
|-- tests/cli.rs          # (planned) integration tests over local bare repos
|-- .cargo/config.toml    # musl targets link via bundled rust-lld
|-- rust-toolchain.toml   # Compiler pinned to 1.96.1
|-- Cargo.lock            # Committed; this is a binary
`-- .github/workflows/ci.yml
```

## WHERE TO LOOK

| Task | Location |
| --- | --- |
| Add a command or flag | `src/main.rs`, then `HELP` in the same file |
| Change argument parsing behaviour | `src/args.rs` |
| Change an exit code | `src/error.rs` |
| Change terminal output or colour rules | `src/ui.rs` |
| Change the size limit or CI steps | `.github/workflows/ci.yml` |
| Change how musl links | `.cargo/config.toml` |
| Bump the compiler | `rust-toolchain.toml` **and** `rust-version` in `Cargo.toml` |

## COMMANDS

```sh
cargo build --release                 # ~300 KB on macOS arm64
cargo test --all
cargo clippy --all-targets -- -D warnings
cargo fmt --all --check

# Fully static Linux binaries, cross-compiled from any host.
# No zig, no Docker, no musl-gcc.
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

Check the size after any change that touches dependencies or generics:

```sh
cargo build --release && ls -l target/release/agent-repos
```

## CONVENTIONS

- `pub(crate)`, not `pub`. This is a binary crate, and `unreachable_pub` is on.
- `#[expect(lint, reason = "...")]`, not `#[allow(...)]`. `expect` warns once
  the suppression stops being needed, so it removes itself instead of rotting.
- Diagnostics go to **stderr** via `ui::log` / `ui::error`. **stdout** is
  reserved for machine-readable output such as `list --json`.
- Colour only on a TTY, and never when `NO_COLOR` is set.
- Do not add a helper before something calls it. An uncalled function fails
  `-D warnings`, and suppressing that costs more than waiting.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 2 | Usage error: unknown option, missing value, mutually exclusive flags |
| 3 | Command not implemented yet (scaffolding; goes away as commands land) |

`sync --check` will exit 1 on drift once it lands.

## PINNING

Everything is pinned to an exact version. Nothing floats.

- **Toolchain** — `rust-toolchain.toml` pins `1.96.1`. rustup honours it
  automatically, so CI needs no third-party toolchain action.
- **Cargo** — `Cargo.lock` is committed. Any dependency ever added goes in as
  `=x.y.z`, never a caret range. Prefer adding nothing.
- **Actions** — pinned to full commit SHAs with the tag in a trailing comment.
  Note that some upstreams publish *annotated* tags, whose tag-object SHA does
  not resolve in `uses:`. Dereference to the commit:
  `gh api repos/OWNER/REPO/git/tags/<tag-sha> --jq .object.sha`
- **Runner images** — exact labels (`ubuntu-24.04`), never `-latest`. A rolling
  label can break the size guard with no code change.

## WORKFLOW

Work ships as stacked PRs via `gh stack`. One branch is one PR.

```sh
gh stack view                  # where am I
gh stack add <branch>          # new branch on top
gh stack submit --auto         # push and create/update PRs (drafts)
gh stack submit --open         # mark ready for review
```

- Every branch must build, lint and test **on its own**. Each one is a PR
  somebody reads in isolation.
- Keep a stack to three branches. Merge it before starting the next — deep
  stacks are painful to rebase when review changes land at the bottom.
- `gh stack modify` and `gh stack rebase` rewrite history across branches. Only
  with a clean worktree.
- `gh stack unstack` deletes the stack **on GitHub as well as locally**. Not a
  local cleanup tool.

## STATUS

The CLI surface is scaffolded: every command parses and validates its flags,
then exits 3. The commands themselves land in later stacks — manifest and git
operations, then update/sync, then status and release packaging.

Once `sync` works, this file should grow its own `<!-- agent-repos:guidance -->`
block and be maintained by the tool itself.
