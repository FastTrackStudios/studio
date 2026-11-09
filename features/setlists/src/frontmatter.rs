//! A note's YAML frontmatter: split from its body, read into a type,
//! written back.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// `text` as (frontmatter, body). No frontmatter: `("", text)`.
#[must_use]
pub fn split(text: &str) -> (&str, &str) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return ("", text);
    };
    let mut at = 0_usize;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let yaml = rest.get(..at).unwrap_or_default();
            let body = rest
                .get(at.saturating_add(line.len())..)
                .unwrap_or_default();
            return (yaml, body);
        }
        at = at.saturating_add(line.len());
    }
    ("", text)
}

/// The frontmatter of `text` as `T`; None when it has none or it does
/// not read as `T`.
#[must_use]
pub fn read<T: DeserializeOwned>(text: &str) -> Option<T> {
    let (yaml, _) = split(text);
    if yaml.trim().is_empty() {
        return None;
    }
    serde_yaml::from_str(yaml).ok()
}

/// A note: `fields` as its frontmatter, then `body`.
#[must_use]
pub fn write<T: Serialize>(fields: &T, body: &str) -> String {
    let yaml = serde_yaml::to_string(fields).unwrap_or_default();
    format!("---\n{yaml}---\n{body}")
}

/// `[[target|label]]`, or `[[target]]` when they are the same.
#[must_use]
pub fn wikilink(target: &str, label: &str) -> String {
    if label.is_empty() || target == label {
        format!("[[{target}]]")
    } else {
        format!("[[{target}|{label}]]")
    }
}

/// The target of `[[target|label]]` (or of a bare `target`).
#[must_use]
pub fn link_target(link: &str) -> &str {
    let inner = link
        .trim()
        .trim_start_matches('!')
        .trim_start_matches("[[")
        .trim_end_matches("]]");
    inner.split('|').next().unwrap_or(inner).trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter_from_the_body() {
        assert_eq!(split("---\na: 1\n---\n# T\n"), ("a: 1\n", "# T\n"));
        assert_eq!(split("# T\n"), ("", "# T\n"));
        assert_eq!(split("---\nno end\n"), ("", "---\nno end\n"));
    }

    #[test]
    fn link_targets() {
        assert_eq!(link_target("[[A/B|B]]"), "A/B");
        assert_eq!(link_target("![[x.svg]]"), "x.svg");
        assert_eq!(link_target("A/B"), "A/B");
        assert_eq!(wikilink("A/B", "B"), "[[A/B|B]]");
    }
}
