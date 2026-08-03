//! Integration tests driving the real binary through its command interface.

#[path = "support/cli.rs"]
mod support;

#[path = "commands/add.rs"]
mod add;
#[path = "cli/arguments.rs"]
mod arguments;
#[path = "cli/entry_errors.rs"]
mod entry_errors;
#[path = "commands/init.rs"]
mod init;
#[path = "cli/interface.rs"]
mod interface;
#[path = "commands/list.rs"]
mod list;
#[path = "commands/pin.rs"]
mod pin;
#[path = "commands/remove.rs"]
mod remove;
#[path = "commands/restore.rs"]
mod restore;
#[path = "commands/status.rs"]
mod status;
#[path = "commands/sync.rs"]
mod sync;
#[path = "commands/update.rs"]
mod update;
