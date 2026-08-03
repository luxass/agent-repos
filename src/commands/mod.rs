//! One module per command, matching the CLI surface one to one.
//!
//! [`crate::cli`] parses arguments and calls exactly one of these. This file is
//! registration only: work that two commands share belongs with the thing it
//! operates on — the manifest, git, or the instruction files — not here.

mod add;
mod init;
mod list;
mod pin;
mod remove;
mod restore;
mod status;
mod update;

pub(crate) use add::{AddRequest, RefSpec, add};
pub(crate) use init::init;
pub(crate) use list::list;
pub(crate) use pin::pin;
pub(crate) use remove::remove;
pub(crate) use restore::restore;
pub(crate) use status::status;
pub(crate) use update::{UpdateRequest, update};
