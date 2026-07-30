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
| Binary stays under 640 KB | Size guard in `.github/workflows/ci.yml` | The reason the project exists — see SIZE below |
| No `unsafe` | `unsafe_code = "forbid"` in `Cargo.toml` | Nothing here needs it |
| Shell out to `git`, never link a git library | Code review | `gix` would add 5-8 MB and mean reimplementing credential handling. Spawning `git` inherits SSH keys, credential helpers, proxies, `GH_TOKEN` and git-lfs for free |
| No C dependencies and no `build.rs` | Code review | This is what lets musl targets cross-compile from any host with no zig, Docker or musl-gcc |
| Every version pinned exactly | `rust-toolchain.toml`, `Cargo.lock`, SHA-pinned actions | See PINNING below |
| A ref is never inferred from `package.json` or a lockfile | Code review | The user pins deliberately, or the default branch's current head commit is pinned |

If a change pushes the binary over the limit, do not raise the limit. Work out
what pulled the weight in and remove it.

## SIZE

Measured with the release profile, `x86_64-unknown-linux-musl` being the
largest target:

| | Bytes |
| --- | --- |
| hello-world using only args, `Command` and `fs` | ~426,000 |
| agent-repos, complete | ~563,000 |
| Guard | 655,360 |

The floor is not our code. `std` links the backtrace machinery — `gimli`,
`addr2line`, `miniz_oxide`, `libunwind`, `rustc_demangle` — whether or not the
program ever panics, and `panic = "abort"` does not remove it. Everything
agent-repos does adds roughly 137 KB on top.

That floor is only escapable with `-Z build-std` plus `panic_immediate_abort`,
which is nightly-only and would break the pinned stable toolchain. If that
trade ever looks worthwhile it is a deliberate decision, not a quiet one.

So the guard exists to catch **our** growth, not to chase the floor. Roughly
80 KB of headroom. If a change eats into it, find out what did:

```sh
cargo build --release --target x86_64-unknown-linux-musl \
  --config 'profile.release.strip="none"'
nm --print-size --size-sort --radix=d \
  target/x86_64-unknown-linux-musl/release/agent-repos | tail -25
```

## STRUCTURE

```text
agent-repos/
|-- src/
|   |-- main.rs           # Module wiring and the exit-code path. Keep it tiny
|   |-- cli.rs            # argv -> a typed call: help, dispatch, flag parsing
|   |-- args.rs           # Hand-rolled argument parser
|   |-- error.rs          # Error type and ExitCode
|   |-- ui.rs             # stderr logging, colour, confirmation prompts
|   |-- manifest.rs       # .agent-repos TOML subset parse/write
|   |-- git.rs            # std::process::Command wrappers around git
|   |-- repo.rs           # init/add/update/remove/restore/pin/status/list
|   |-- sync.rs           # AGENTS.md block scanning and rewriting
|   |-- render.rs         # Block body generation
|   |-- completions.rs    # fish/bash/zsh completion scripts
|   |-- paths.rs          # Relative-path validation, containment
|   `-- fsx.rs            # Atomic writes
|-- tests/cli.rs          # Integration tests over local git fixtures
|-- .cargo/config.toml    # musl targets link via bundled rust-lld
|-- rust-toolchain.toml   # Compiler pinned to 1.96.1
|-- Cargo.lock            # Committed; this is a binary
`-- .github/workflows/ci.yml
```

## WHERE TO LOOK

| Task | Location |
| --- | --- |
| Add a command or flag | `src/cli.rs` — the command table, its parser, and `HELP` — then `src/completions.rs` |
| Change argument parsing behaviour | `src/args.rs` |
| Change an exit code | `src/error.rs` |
| Change terminal output, colour or prompts | `src/ui.rs` |
| Change the manifest format | `src/manifest.rs` (bump `FORMAT_VERSION` if breaking) |
| Change what a git operation does | `src/git.rs` |
| Change command behaviour | `src/repo.rs` |
| Add or change a generated block | `src/render.rs`, then the dispatch in `src/sync.rs` |
| Change the size limit or CI steps | `.github/workflows/ci.yml` |
| Change how musl links | `.cargo/config.toml` |
| Bump the compiler | `rust-toolchain.toml` **and** `rust-version` in `Cargo.toml` |

## COMMANDS

```sh
cargo build --release                 # ~421 KB on macOS arm64
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
- `main.rs` stays wiring only. Argument handling belongs in `cli.rs`, work
  belongs in `repo.rs` or `sync.rs`. If you find yourself adding a `use` in the
  middle of a file to make an edit fit, the edit is in the wrong file.
- More than two same-typed parameters in a row is a transposition waiting to
  happen — `AddRequest` and `UpdateRequest` exist for exactly that reason. Two
  booleans that cannot both be true want an enum, as `SyncMode` does.

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | The command could not be carried out: bad manifest, git failure, missing entry. Also `sync --check` when a file is out of date |
| 2 | Usage error: unknown option, missing value, mutually exclusive flags |

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

## TESTING

Unit tests live beside the code they cover. Integration tests in `tests/cli.rs`
drive the real binary against **local** git fixtures built with `git init` —
never the network, so they work offline and in CI.

When adding a command, cover the refusal paths too, not just the happy one.
Most of the value in this tool is that it declines to do the wrong thing:
deleting something that is not a checkout, moving a pin nobody asked to move,
writing a file it could not fully render.

## STATUS

Every command is implemented. This repository does not use `agent-repos` on
itself — it has no external dependencies to vendor — so there are no
`<!-- agent-repos:... -->` blocks in this file. See the README for what the
generated blocks look like.
