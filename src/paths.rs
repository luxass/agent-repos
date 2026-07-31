//! Path validation.
//!
//! Every path in the manifest is relative to the repository root and is used
//! to create — and eventually delete — directories. Validating them up front
//! is what keeps `remove` from being talked into touching something outside
//! the clone directory.

use crate::error::{Error, Result};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_relative_paths_are_accepted() {
        for path in ["repos/effect", "repos/a/b/c", "vendor", "a-b_c.d"] {
            assert!(validate_relative("path", path).is_ok(), "{path}");
        }
    }

    #[test]
    fn absolute_paths_are_rejected() {
        for path in ["/etc/passwd", "/", "C:/Windows", "\\\\server\\share"] {
            assert!(validate_relative("path", path).is_err(), "{path}");
        }
    }

    #[test]
    fn dot_segments_are_rejected() {
        for path in [
            "..",
            ".",
            "../evil",
            "repos/../../evil",
            "repos/..",
            "./repos",
            "repos\\..\\evil",
        ] {
            assert!(validate_relative("path", path).is_err(), "{path}");
        }
    }

    #[test]
    fn empty_segments_and_control_characters_are_rejected() {
        for path in ["", "repos//effect", "repos/\tx", "repos/\nx"] {
            assert!(validate_relative("path", path).is_err(), "{path:?}");
        }
    }

    #[test]
    fn containment_compares_whole_segments() {
        assert!(is_inside("repos", "repos/effect"));
        assert!(is_inside("repos", "repos/a/b"));
        assert!(is_inside("repos/", "repos/effect"));

        assert!(!is_inside("repos", "repos"));
        assert!(!is_inside("repos", "repos-evil/x"));
        assert!(!is_inside("repos", "vendor/effect"));
        assert!(!is_inside("repos", "elsewhere"));
    }
}
