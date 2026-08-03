//! Shell completion scripts, emitted on stdout for the user to source.

use crate::error::{Error, Result};

/// Resolves a shell name to its script. Knowing which shells exist belongs
/// here, next to the scripts, rather than in the argument parser.
pub(crate) fn script(shell: &str) -> Result<String> {
    match shell {
        "fish" => Ok(fish()),
        "bash" => Ok(bash()),
        "zsh" => Ok(zsh()),
        other => Err(Error::usage(format!(
            "unsupported shell `{other}` (expected fish, bash or zsh)"
        ))),
    }
}

/// Commands and their one-line descriptions, shared by every shell.
const COMMANDS: &[(&str, &str)] = &[
    ("init", "Prepare this repository for reference repos"),
    ("add", "Add a reference repository pinned to a ref"),
    ("update", "Re-check, repoint or advance a pin"),
    ("restore", "Clone anything missing at its pinned ref"),
    ("remove", "Remove an entry"),
    ("list", "List configured reference repositories"),
    ("status", "Report drift, local edits and missing clones"),
    ("pin", "Freeze an entry to the checked-out commit"),
    ("sync", "Refill the blocks in AGENTS.md / CLAUDE.md"),
    ("completions", "Emit a shell completion script"),
    ("help", "Show help"),
    ("version", "Show the version"),
];

/// Commands whose first argument is an entry name.
const NAME_TAKING: &[&str] = &["update", "remove", "pin"];

fn fish() -> String {
    let mut out = String::from(
        "# agent-repos completions for fish\n\
         # Install: agent-repos completions fish > ~/.config/fish/completions/agent-repos.fish\n\
         \n\
         function __agent_repos_names\n    \
             set -l root (git rev-parse --show-toplevel 2>/dev/null)\n    \
             test -n \"$root\"; and test -f \"$root/.agent-repos/manifest.toml\"; or return\n    \
             string match -rg '^name = \"(.*)\"$' < \"$root/.agent-repos/manifest.toml\"\n\
         end\n\
         \n\
         complete -c agent-repos -f\n",
    );

    for (command, description) in COMMANDS {
        out.push_str(&format!(
            "complete -c agent-repos -n __fish_use_subcommand -a {command} -d '{description}'\n"
        ));
    }

    out.push_str(&format!(
        "\ncomplete -c agent-repos -n '__fish_seen_subcommand_from {}' \
         -a '(__agent_repos_names)' -d 'Reference repository'\n",
        NAME_TAKING.join(" ")
    ));

    out.push_str(
        "\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from add' -l tag -r -d 'Pin to a tag'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from add' -l branch -r -d 'Follow a branch'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from add' -l commit -r -d 'Pin to a commit'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from add' -s n -l name -r -d 'Entry name'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from add' -s p -l path -r -d 'Checkout path'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from add' -l desc -r -d 'Why this repo is here'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from add' -l use -r -d 'What to consult it for'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from add' -l no-sync -d 'Skip refreshing instructions'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from update' -l to -r -d 'Repoint to a ref'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from update' -l latest -d 'Advance to the newest'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from update' -s a -l all -d 'Every entry'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from update remove' -s y -l yes -d 'Do not ask'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from remove' -l keep-files -d 'Keep the checkout'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from list' -l json -d 'Machine-readable output'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from init sync' -l target -r -d 'Instruction file'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from init' -l dir -r -d 'Clone directory'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from init' -l no-instructions -d 'Manage no files'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from sync' -l check -d 'Exit 1 if out of date'\n\
         complete -c agent-repos -n '__fish_seen_subcommand_from completions' -a 'fish bash zsh'\n",
    );

    out
}

fn bash() -> String {
    let commands: Vec<&str> = COMMANDS.iter().map(|(name, _)| *name).collect();

    format!(
        "# agent-repos completions for bash\n\
         # Install: agent-repos completions bash > /etc/bash_completion.d/agent-repos\n\
         \n\
         _agent_repos() {{\n    \
             local cur prev commands names root\n    \
             cur=\"${{COMP_WORDS[COMP_CWORD]}}\"\n    \
             prev=\"${{COMP_WORDS[1]}}\"\n    \
             commands=\"{commands}\"\n\
         \n    \
             if [ \"$COMP_CWORD\" -eq 1 ]; then\n        \
                 COMPREPLY=($(compgen -W \"$commands\" -- \"$cur\"))\n        \
                 return\n    \
             fi\n\
         \n    \
             case \"$prev\" in\n        \
                 {name_taking})\n            \
                     root=$(git rev-parse --show-toplevel 2>/dev/null)\n            \
                     if [ -n \"$root\" ] && [ -f \"$root/.agent-repos/manifest.toml\" ]; then\n                \
                         names=$(sed -n 's/^name = \"\\(.*\\)\"$/\\1/p' \"$root/.agent-repos/manifest.toml\")\n                \
                         COMPREPLY=($(compgen -W \"$names\" -- \"$cur\"))\n            \
                     fi\n            \
                     ;;\n        \
                 completions)\n            \
                     COMPREPLY=($(compgen -W \"fish bash zsh\" -- \"$cur\"))\n            \
                     ;;\n        \
                 *)\n            \
                     COMPREPLY=($(compgen -W \"--help --version\" -- \"$cur\"))\n            \
                     ;;\n    \
             esac\n\
         }}\n\
         complete -F _agent_repos agent-repos\n",
        commands = commands.join(" "),
        name_taking = NAME_TAKING.join("|"),
    )
}

fn zsh() -> String {
    let mut descriptions = String::new();
    for (command, description) in COMMANDS {
        descriptions.push_str(&format!("        '{command}:{description}'\n"));
    }

    format!(
        "#compdef agent-repos\n\
         # agent-repos completions for zsh\n\
         # Install: agent-repos completions zsh > \"${{fpath[1]}}/_agent-repos\"\n\
         \n\
         _agent_repos_names() {{\n    \
             local root\n    \
             root=$(git rev-parse --show-toplevel 2>/dev/null) || return\n    \
             [[ -f $root/.agent-repos/manifest.toml ]] || return\n    \
             sed -n 's/^name = \"\\(.*\\)\"$/\\1/p' \"$root/.agent-repos/manifest.toml\"\n\
         }}\n\
         \n\
         _agent_repos() {{\n    \
             local -a commands\n    \
             commands=(\n{descriptions}    )\n\
         \n    \
             if (( CURRENT == 2 )); then\n        \
                 _describe -t commands 'agent-repos command' commands\n        \
                 return\n    \
             fi\n\
         \n    \
             case ${{words[2]}} in\n        \
                 {name_taking})\n            \
                     compadd $(_agent_repos_names)\n            \
                     ;;\n        \
                 completions)\n            \
                     compadd fish bash zsh\n            \
                     ;;\n    \
             esac\n\
         }}\n\
         \n\
         _agent_repos \"$@\"\n",
        name_taking = NAME_TAKING.join("|"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_appears_in_every_script() {
        let scripts = [fish(), bash(), zsh()];
        for (command, _) in COMMANDS {
            for script in &scripts {
                assert!(script.contains(command), "{command} missing from a script");
            }
        }
    }

    #[test]
    fn scripts_are_not_accidentally_empty() {
        for script in [fish(), bash(), zsh()] {
            assert!(script.lines().count() > 10);
        }
    }

    #[test]
    fn zsh_declares_the_compdef_tag_first() {
        assert!(zsh().starts_with("#compdef agent-repos\n"));
    }

    #[test]
    fn name_taking_commands_offer_entry_names() {
        for command in NAME_TAKING {
            assert!(fish().contains(command));
            assert!(bash().contains(command));
        }
    }
}
