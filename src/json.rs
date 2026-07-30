//! `list --json` output.
//!
//! Hand-rolled rather than pulling in serde, which would cost more than the
//! rest of the binary. The only escaping that matters is done in
//! [`json_escape`]; everything else is fixed-shape.

use std::path::Path;

use crate::manifest::Manifest;

pub(crate) fn render(manifest: &Manifest, root: &Path) -> String {
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"dir\": \"{}\",\n", json_escape(&manifest.dir)));

    out.push_str("  \"targets\": [");
    for (index, target) in manifest.targets.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("\"{}\"", json_escape(target)));
    }
    out.push_str("],\n");

    out.push_str("  \"repos\": [");
    for (index, repo) in manifest.repos.iter().enumerate() {
        out.push_str(if index > 0 { ",\n    {\n" } else { "\n    {\n" });
        out.push_str(&format!(
            "      \"name\": \"{}\",\n",
            json_escape(&repo.name)
        ));
        out.push_str(&format!("      \"url\": \"{}\",\n", json_escape(&repo.url)));
        out.push_str(&format!(
            "      \"ref\": \"{}\",\n",
            json_escape(&repo.git_ref)
        ));
        out.push_str(&format!("      \"kind\": \"{}\",\n", repo.kind.as_str()));
        out.push_str(&format!(
            "      \"path\": \"{}\",\n",
            json_escape(&repo.path)
        ));
        out.push_str(&format!(
            "      \"track\": {},\n",
            json_option(repo.track.as_deref())
        ));
        out.push_str(&format!(
            "      \"desc\": {},\n",
            json_option(repo.desc.as_deref())
        ));
        out.push_str(&format!(
            "      \"use\": {},\n",
            json_option(repo.usage.as_deref())
        ));
        out.push_str(&format!(
            "      \"present\": {}\n",
            root.join(&repo.path).exists()
        ));
        out.push_str("    }");
    }
    out.push_str(if manifest.repos.is_empty() {
        "]\n"
    } else {
        "\n  ]\n"
    });
    out.push_str("}\n");
    out
}

fn json_option(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", json_escape(value)),
        None => "null".to_string(),
    }
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", ch as u32)),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Kind, Repo};

    fn manifest_with(repos: Vec<Repo>) -> Manifest {
        let mut manifest = Manifest::new("repos".to_string(), vec!["AGENTS.md".to_string()]);
        manifest.repos = repos;
        manifest
    }

    fn repo(name: &str, kind: Kind) -> Repo {
        Repo {
            name: name.to_string(),
            url: format!("https://example.com/{name}"),
            git_ref: "v1.0.0".to_string(),
            kind,
            path: format!("repos/{name}"),
            track: None,
            desc: None,
            usage: None,
            comments: Vec::new(),
        }
    }

    #[test]
    fn json_escapes_control_characters() {
        assert_eq!(json_escape("a\"b\\c\nd\te"), "a\\\"b\\\\c\\nd\\te");
        assert_eq!(json_escape("\u{1}"), "\\u0001");
    }
    #[test]
    fn json_renders_null_for_absent_fields() {
        let manifest = manifest_with(vec![repo("effect", Kind::Tag)]);
        let json = render(&manifest, Path::new("/nonexistent"));

        assert!(json.contains("\"track\": null"));
        assert!(json.contains("\"desc\": null"));
        assert!(json.contains("\"present\": false"));
        assert!(json.contains("\"kind\": \"tag\""));
    }
    #[test]
    fn json_with_no_repos_is_still_well_formed() {
        let json = render(&manifest_with(Vec::new()), Path::new("/nonexistent"));
        assert!(json.contains("\"repos\": []"));
        assert!(json.trim_end().ends_with('}'));
    }
    #[test]
    fn json_separates_multiple_repos() {
        let manifest = manifest_with(vec![repo("a", Kind::Tag), repo("b", Kind::Branch)]);
        let json = render(&manifest, Path::new("/nonexistent"));
        assert_eq!(json.matches("\"name\":").count(), 2);
        assert!(json.contains("},\n    {"));
    }
}
