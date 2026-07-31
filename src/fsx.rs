//! Filesystem helpers.

use std::fs;
use std::path::Path;

use crate::error::{Error, Result};

/// Writes via a temporary file in the same directory, then renames.
///
/// A rename within a directory is atomic, so a reader never observes a
/// half-written manifest or a truncated `AGENTS.md`, and an interrupted run
/// leaves the original intact. The temporary file is removed if the rename
/// fails.
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

    fs::write(&temp, contents)
        .map_err(|err| Error::failure(format!("could not write {}: {err}", temp.display())))?;

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

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("agent-repos-fsx-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_and_replaces() {
        let dir = scratch("replace");
        let file = dir.join("out.txt");

        write_atomic(&file, "first").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "first");

        write_atomic(&file, "second").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "second");

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn leaves_no_temporary_files_behind() {
        let dir = scratch("clean");
        write_atomic(&dir.join("out.txt"), "hello").unwrap();

        let entries: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(entries, vec!["out.txt"]);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn creates_missing_parent_directories() {
        let dir = scratch("nested");
        let file = dir.join("a").join("b").join("out.txt");

        write_atomic(&file, "deep").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "deep");

        fs::remove_dir_all(&dir).unwrap();
    }
}
