# agent-repos

`agent-repos` keeps reference repositories locally for coding agents to read.
Each copy uses a Git tag, branch, or commit (a ref).
The tool records these choices (pins) in `.agent-repos/manifest.toml`.
It updates marked sections in `AGENTS.md` and `CLAUDE.md`.
Git ignores the local copies.

## Build

Use Rust 1.96.1 and make sure that Git is on `PATH`.

```sh
cargo build --locked
cargo test --locked
```

Run the development build:

```sh
cargo run -- --help
cargo run -- version
```

## Usage

| Invocation | Behavior |
|------------|----------|
| `agent-repos init` | Create `.agent-repos/manifest.toml` and instruction sections |
| `agent-repos add <url>` | Copy a repository and record its pin |
| `agent-repos update <name>...` | Repair selected copies or change their pins |
| `agent-repos update --all --latest` | Move every entry to its latest available ref |
| `agent-repos restore` | Create missing local copies |
| `agent-repos status` | Compare the configuration, local copies, and instruction files |
| `agent-repos list --json` | Print configured repositories as JSON |
| `agent-repos sync --check` | Exit 1 when generated sections are out of date |
| `agent-repos remove <name>` | Remove an entry and its local copy |
| `agent-repos pin <name>` | Pin an entry to its current commit |

For `add`, use a Git URL, local path, SCP-style SSH address, or repository shorthand:

```sh
agent-repos add github:Effect-TS/effect --tag v3.12.0
agent-repos add gitlab:group/repo --branch main
agent-repos add git.example.com:owner/repo --commit 9f3a1c2
agent-repos add github:better-auth/better-auth --use \
  'When working with Better Auth, inspect `.agent-repos/repos/better-auth/` first.'
```

Without a ref flag, `add` pins the default branch's current commit.
`agent-repos` never chooses a ref from a package manifest or lockfile.

Use these flags:

- Select one of `--tag`, `--branch`, or `--commit`.
- Use `--name` and `--path` to set the name and local path.
- Use `--desc` to explain why the repository is present.
- Use `--use` to tell agents when and how to read it.
- Use `update --to REF` to select a ref.
- Use `update --latest` to find the newest tag or tracked branch commit.
- Use `--yes` to accept update and removal prompts.
- Use `--no-sync` to add without updating instruction files.

## Configuration

Commit `.agent-repos/manifest.toml` to share the pins and instruction file paths.
Local copies default to `.agent-repos/repos/`, which Git ignores.

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
use = "When writing Effect code, inspect `.agent-repos/repos/effect/` for idiomatic APIs, tests, and module structure."
```

Instruction files can contain marked sections:

```markdown
<!-- agent-repos:guidance -->
<!-- /agent-repos:guidance -->

<!-- agent-repos:repos fields=name,ref,path,desc -->
<!-- /agent-repos:repos -->
```

`agent-repos sync` updates these sections and leaves all other text unchanged.
Each non-empty `use` value becomes a bullet with the repository name:

```markdown
- **effect** — When writing Effect code, inspect `.agent-repos/repos/effect/` for idiomatic APIs, tests, and module structure.
```

The tool copies your Markdown unchanged, without substituting paths.
Edit `use` in `.agent-repos/manifest.toml`.
Then run `agent-repos sync` to update all configured instruction files.

## Development

Run all repository checks with:

```sh
just ci
```

The release binary has no dependencies and must stay below 655,360 bytes.
On Linux musl targets, it needs no shared libraries.
Before contributing, read [AGENTS.md](AGENTS.md) for the code structure and repository rules.
