//! Ordering version-shaped tags, for `update --latest`.
//!
//! Deliberately not a semver implementation: it only has to answer "which of
//! these tags is newest", and it has to get `v10` > `v9` right, which a string
//! comparison does not.

/// Orders a tag for "which of these is newest".
///
/// Returns the numeric components plus whether it is a stable release, so that
/// `v1.2.0` sorts above `v1.2.0-rc.1`. Tags that are not version-shaped return
/// `None` and are ignored when picking the newest, rather than being ordered
/// by string comparison, where `v10` would lose to `v9`.
pub(crate) fn version_key(tag: &str) -> Option<(Vec<u64>, bool)> {
    let core = tag
        .strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag);

    let (numbers, suffix) = match core.find(['-', '+']) {
        Some(index) => (&core[..index], &core[index..]),
        None => (core, ""),
    };

    let parts: Option<Vec<u64>> = numbers
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect();

    let parts = parts?;
    if parts.is_empty() {
        return None;
    }
    Some((parts, suffix.is_empty()))
}

/// The highest version-shaped tag, or `None` if none of them look like one.
pub(crate) fn newest_tag(tags: &[String]) -> Option<&String> {
    tags.iter()
        .filter_map(|tag| version_key(tag).map(|key| (key, tag)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, tag)| tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn versions_order_numerically_not_lexically() {
        // The whole reason for parsing: "v9" > "v10" as strings.
        let list = tags(&["v9.0.0", "v10.0.0", "v2.0.0"]);
        assert_eq!(newest_tag(&list).map(String::as_str), Some("v10.0.0"));
    }
    #[test]
    fn a_stable_release_beats_its_prereleases() {
        let list = tags(&["v1.2.0-rc.1", "v1.2.0", "v1.2.0-beta"]);
        assert_eq!(newest_tag(&list).map(String::as_str), Some("v1.2.0"));
    }
    #[test]
    fn a_prerelease_still_wins_if_it_is_the_highest_version() {
        let list = tags(&["v1.2.0", "v1.3.0-rc.1"]);
        assert_eq!(newest_tag(&list).map(String::as_str), Some("v1.3.0-rc.1"));
    }
    #[test]
    fn unversioned_tags_are_ignored() {
        let mixed = tags(&["latest", "nightly", "v1.0.0"]);
        assert_eq!(newest_tag(&mixed).map(String::as_str), Some("v1.0.0"));
        let none = tags(&["latest", "stable"]);
        assert_eq!(newest_tag(&none), None);
        assert_eq!(newest_tag(&[]), None);
    }
    #[test]
    fn the_v_prefix_is_optional() {
        assert_eq!(version_key("v1.2.3"), version_key("1.2.3"));
        assert_eq!(
            newest_tag(&tags(&["1.0.0", "2.0.0"])).map(String::as_str),
            Some("2.0.0")
        );
    }
    #[test]
    fn version_key_rejects_non_versions() {
        assert!(version_key("latest").is_none());
        assert!(version_key("v1.x.0").is_none());
        assert!(version_key("").is_none());
        assert!(version_key("release-2024").is_none());
    }
}
