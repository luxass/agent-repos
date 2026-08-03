# AGENT-REPOS

`agent-repos` maintains pinned clones of external repositories under
`.agent-repos/repos/`, so coding agents can read the exact source a project
depends on instead of guessing at its API.

It is a Rust CLI with zero dependencies. The product is the small, portable,
statically linked binary—not merely the behavior of the commands.

## A note from Lucas

Hi. I built this because I want agents to inspect real code before confidently
inventing an answer, and I do not think enabling that should require a large
tool or another ecosystem.

I want `agent-repos` to feel almost boring: one tiny binary, a visible desired
state, ordinary Git, and failure modes that leave the project understandable.
Prefer the obvious design over the clever one. “Obvious” does not always mean
less code; a little coordination is worth it when it prevents lost data. But
every abstraction and file should earn its place.

Please push back when a request would weaken those qualities. Preserve the
reason the tool exists, not an implementation merely because it already exists.

— Lucas

## Non-negotiable constraints

Do not break these without explicit maintainer approval.

| Constraint | Why |
| --- | --- |
| Use `std` only; keep `[dependencies]` empty | A dependency such as `clap` costs more than the rest of the binary |
| Keep the release binary below 655,360 bytes | Smallness and portability are product requirements, enforced by CI |
| No `unsafe`, C dependencies, or `build.rs` | Nothing here needs them; musl targets must cross-compile without extra tooling |
| Shell out to `git`; never link a Git library | This preserves users’ SSH keys, credential helpers, proxies, tokens, and Git LFS behavior |
| Pin every toolchain, action, and package version exactly | Builds must not change because something floated |
| Never infer a ref from a package manifest or lockfile | The user pins deliberately, or the default branch’s current head commit is pinned |

If a change crosses the size limit, do not raise the limit. Find and remove the
growth.

## Project shape

| Concern | Location |
| --- | --- |
| Module wiring and exit path | `src/main.rs`; keep it tiny |
| CLI dispatch, help, and command flags | `src/cli/mod.rs`, then update `src/cli/completions.rs` |
| Hand-written argument parsing | `src/cli/args.rs` |
| Command behavior | `src/commands/<command>.rs`; one file per command |
| Command registration | `src/commands/mod.rs`; declarations and re-exports only |
| Manifest format, lookup, and writer locking | `src/manifest.rs`; bump `FORMAT_VERSION` for breaking formats |
| Git subprocess behavior, and how a pin maps to it | `src/git.rs` |
| Generated instruction blocks | Render in `src/render.rs`, dispatch in `src/sync.rs` |
| Terminal diagnostics and prompts | `src/ui.rs` |
| JSON output, versions, and paths | `src/json.rs`, `src/version.rs`, `src/paths.rs` |
| Atomic filesystem writes | `src/fsx.rs` |
| End-to-end CLI behavior | `tests/cli.rs`, using local Git fixtures only |

The on-disk state is deliberately explicit:

```text
.agent-repos/
|-- manifest.toml  # committed desired state
|-- write.lock     # ignored stable lock file
`-- repos/         # ignored reference checkouts
```

Manifest and generated-instruction writes must be atomic and serialized. For
`add`, do slow clone work before taking the writer lock, then reload the
manifest while holding the lock before appending. This is what allows parallel
adds without losing entries.

## Design rules

- Use `pub(crate)`, not `pub`; this is a binary crate and `unreachable_pub` is
  enforced.
- Use `#[expect(lint, reason = "...")]`, not `#[allow(...)]`.
- Keep `main.rs` as wiring. Argument handling belongs in `cli/`; work belongs
  in `commands/`.
- Add a helper only when something calls it. When a second command needs one,
  it goes with the thing it operates on — the manifest, git, or the instruction
  files — not into `commands/mod.rs`, which stays registration only.
- More than two adjacent parameters of the same type need a request struct.
  Mutually exclusive booleans usually need an enum.
- Diagnostics go to stderr through `ui::log` or `ui::error`. Reserve stdout for
  machine-readable output such as `list --json`.
- Use color only on a TTY and never when `NO_COLOR` is set.
- Refusal paths matter as much as happy paths. Check everything needed to
  decline safely before mutating the manifest or deleting files.
- Preserve user-authored manifest comments and all text outside generated
  instruction blocks.

Exit codes are part of the interface:

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Operational failure, including `sync --check` drift |
| 2 | Usage error |

## Verification

During development, run the smallest relevant unit or integration test. Before
considering a branch complete, run:

```sh
cargo fmt --all --check
cargo test --all
cargo clippy --all-targets -- -D warnings
```

Integration tests must use local repositories created with `git init`; they
must never require the network. Cover refusal cases: unsafe deletion, unknown
refs, duplicate entries, malformed blocks, and writes that must remain atomic.

After changes involving dependencies, generics, release settings, or other
likely code-size growth, check the largest target:

```sh
cargo build --release --target x86_64-unknown-linux-musl
wc -c target/x86_64-unknown-linux-musl/release/agent-repos
```

If it grows unexpectedly, inspect symbols instead of guessing:

```sh
cargo build --release --target x86_64-unknown-linux-musl \
  --config 'profile.release.strip="none"'
nm --print-size --size-sort --radix=d \
  target/x86_64-unknown-linux-musl/release/agent-repos | tail -25
```

Keep `rust-toolchain.toml` and `Cargo.toml`’s `rust-version` aligned. Keep
`Cargo.lock` committed, dependency versions exact, GitHub Actions pinned to
full commit SHAs, and runner labels exact rather than `-latest`.

## Shipping

Work ships as stacked PRs through `gh stack`. One branch is one independently
buildable, lintable, testable PR. Keep a stack to at most three branches.

```sh
gh stack view --json
gh stack add <branch>
gh stack submit --auto       # draft PRs
gh stack submit --auto --open
```

Only rebase or modify a stack with a clean worktree. `gh stack unstack` removes
the stack on GitHub as well as locally; it is not a cleanup command.
