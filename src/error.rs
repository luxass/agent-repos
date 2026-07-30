//! Error type and process exit codes.

use std::fmt;

/// Process exit codes. `0` is returned implicitly on success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExitCode {
    /// The command line itself was wrong: unknown option, missing value,
    /// mutually exclusive flags.
    Usage = 2,
    /// Scaffolding only, until the command lands.
    Unimplemented = 3,
}

#[derive(Debug)]
pub(crate) struct Error {
    message: String,
    code: ExitCode,
}

impl Error {
    pub(crate) fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: ExitCode::Usage,
        }
    }

    pub(crate) fn unimplemented(command: &str) -> Self {
        Self {
            message: format!("`{command}` is not implemented yet"),
            code: ExitCode::Unimplemented,
        }
    }

    pub(crate) fn code(&self) -> i32 {
        self.code as i32
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
