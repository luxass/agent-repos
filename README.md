# agent-repos

`agent-repos` maintains pinned clones of reference repositories for coding
agents. It resolves refs through Git, checks out the exact source a project
depends on, and records the desired state in a committed manifest. Generated
blocks keep `AGENTS.md` and `CLAUDE.md` in sync; local clones stay uncommitted.

## Build

agent-repos targets Rust 1.96.1 and requires Git on `PATH`.

```sh
cargo build --locked
cargo test --locked
```

Run the development build with `cargo run --`:

```sh
cargo run -- --help
cargo run -- version
```

## Usage

| Invocation | Behavior |
|------------|----------|
| `agent-repos init` | Create `.agent-repos/manifest.toml` and seed instruction blocks |
| `agent-repos add <url>` | Clone a repository and record an exact pin |
| `agent-repos update <name>...` | Verify, repair, or move selected pins |
| `agent-repos update --all --latest` | Move every entry to its latest available ref |
| `agent-repos restore` | Restore clones missing from disk |
| `agent-repos status` | Compare the manifest, clones, and instruction files |
| `agent-repos list --json` | Print configured repositories as JSON |
| `agent-repos sync --check` | Exit 1 when generated instruction blocks have drifted |
| `agent-repos remove <name>` | Remove an entry and its local clone |
| `agent-repos pin <name>` | Freeze an entry at its checked-out commit |

Pass a full Git URL, local path, SCP-style SSH remote, or forge shorthand to
`add`:

```sh
agent-repos add github:Effect-TS/effect --tag v3.12.0
agent-repos add gitlab:group/repo --branch main
agent-repos add git.example.com:owner/repo --commit 9f3a1c2
```

Command options control how entries are recorded and maintained:

- `--tag`, `--branch`, and `--commit` select the ref kind.
- Omitting a ref pins the default branch's current head commit.
- `--name` and `--path` override the generated name and checkout path.
- `--desc` and `--use` tell agents why and when to consult the repository.
- `update --to REF` moves an entry to a specific ref.
- `update --latest` finds the newest tag or tracked branch head.
- `--yes` accepts update and removal prompts.
- `--no-sync` adds an entry without refreshing instruction files.

Ref options are mutually exclusive. agent-repos never infers a ref from a
package manifest or lockfile.

## Configuration

`.agent-repos/manifest.toml` records the desired state. Local clones live under
`.agent-repos/repos/` by default and are ignored by Git.

```toml
version = 1
dir = ".agent-repos/repos"
targets = ["AGENTS.md", "CLAUDE.md"]

[[repo]]
name = "effect"
url = "https://github.com/Effect-TS/effect"
ref = "v3.12.0"
kind = "tag"
path = ".agent-repos/repos/effect"
desc = "Effect runtime this service is built on"
use = "API signatures, Layer/Runtime composition, test style"
```

Configured instruction files may contain generated blocks such as:

```markdown
<!-- agent-repos:guidance -->
<!-- /agent-repos:guidance -->

<!-- agent-repos:repos fields=name,ref,path,desc -->
<!-- /agent-repos:repos -->
```

`agent-repos sync` refills those blocks while preserving all text outside them.

## Development

Run all repository checks with:

```sh
just ci
```

The release binary has no dependencies, is fully static on Linux musl targets,
and must remain below 655,360 bytes. Contributors and coding agents should also
read [AGENTS.md](AGENTS.md), which records the architecture and repository
conventions.
