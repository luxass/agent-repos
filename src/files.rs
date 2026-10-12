//! The two filesystem rules the rest of the crate depends on: which paths are
//! allowed in the manifest, and how a file is replaced without ever being seen
//! half-written.
//!
//! Every path in the manifest is relative to the repository root and is used
//! to create — and eventually delete — directories. Validating them up front
//! is what keeps `remove` from being talked into touching something outside
//! the clone directory.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::ui::{Error, Result};

/// Rejects anything that is not a plain relative path: absolute paths, `.`,
/// `..` in any position, empty or repeated separators, and control characters
/// that would corrupt the manifest.
pub(crate) fn validate_relative(label: &str, path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::failure(format!("{label} must not be empty")));
    }
    if path.contains(['\n', '\r', '\t']) {
        return Err(Error::failure(format!(
            "{label} must not contain tabs or newlines: {path:?}"
        )));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(Error::failure(format!(
            "{label} must be relative, not absolute: {path}"
        )));
    }
    // A Windows drive letter or UNC prefix is absolute too.
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return Err(Error::failure(format!(
            "{label} must be relative, not absolute: {path}"
        )));
    }

    for component in path.split(['/', '\\']) {
        match component {
            "" => {
                return Err(Error::failure(format!(
                    "{label} must not contain empty path segments: {path}"
                )));
            }
            "." | ".." => {
                return Err(Error::failure(format!(
                    "{label} must not contain '{component}' segments: {path}"
                )));
            }
            _ => {}
        }
    }

    Ok(())
}

/// True when `path` sits underneath `dir`, comparing whole segments so that
/// `repos-evil/x` does not count as being inside `repos`.
pub(crate) fn is_inside(dir: &str, path: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    let mut dir_parts = dir.split('/');
    let mut path_parts = path.split('/');

    loop {
        match (dir_parts.next(), path_parts.next()) {
            // Every segment of dir matched, and path has more to go.
            (None, Some(_)) => return true,
            (None, None) => return false, // identical, not *inside*
            (Some(_), None) => return false,
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => return false,
        }
    }
}

/// Writes via a temporary file in the same directory, then renames.
///
/// A rename within a directory is atomic, so a reader never observes a
/// half-written manifest or a truncated `AGENTS.md`, and an interrupted run
/// leaves the original intact. The temporary file is created exclusively and
/// removed if writing or renaming fails.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("agent-repos");

    if !dir.as_os_str().is_empty() {
        fs::create_dir_all(dir)
            .map_err(|err| Error::failure(format!("could not create {}: {err}", dir.display())))?;
    }

    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|err| Error::failure(format!("could not create {}: {err}", temp.display())))?;
    if let Err(err) = file.write_all(contents.as_bytes()) {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(Error::failure(format!(
            "could not write {}: {err}",
            temp.display()
        )));
    }
    drop(file);

    // Keep the mode of the file being replaced, so an executable or
    // group-writable target does not silently change permissions.
    #[cfg(unix)]
    if let Ok(existing) = fs::metadata(path) {
        use std::os::unix::fs::PermissionsExt;
        let mode = existing.permissions().mode();
        let _ = fs::set_permissions(&temp, fs::Permissions::from_mode(mode));
    }

    if let Err(err) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(Error::failure(format!(
            "could not replace {}: {err}",
            path.display()
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_leaves_an_occupied_temporary_path_untouched() {
        let dir =
            std::env::temp_dir().join(format!("agent-repos-atomic-write-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("target.txt");
        let temp = dir.join(format!(".target.txt.{}.tmp", std::process::id()));
        fs::write(&target, "original\n").unwrap();
        fs::write(&temp, "other data\n").unwrap();

        assert!(write_atomic(&target, "replacement\n").is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "original\n");
        assert_eq!(fs::read_to_string(&temp).unwrap(), "other data\n");

        fs::remove_file(&temp).unwrap();
        write_atomic(&target, "replacement\n").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "replacement\n");
        assert!(!temp.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
