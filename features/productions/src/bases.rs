//! The default bases: the status views of every production.
//!
//! The pinned Architect has no way for an extension to ship or pin a
//! `.base` (`architect:features/extensions/src/contrib.rs` `Contribution`
//! has `base_views`, view *types*, and no bases), so the scan writes
//! these as files through the host (`VAULT_CREATE`), once: one that is
//! already there is a person's and is left alone.
//!
//! - **By stage** is a board: its first view is `type: kanban`, the
//!   Tasks extension's view type (a column per `stage`; a card dragged to
//!   another column is a property edit), its second a table grouped by
//!   stage for an app without that view type.
//! - **Changed this week** filters on `changedThisWeek`, which the scan
//!   stamps: the pinned base engine has `now()` and no date arithmetic
//!   (`architect:features/vault/vault-live/src/bases.rs`), so "saved in
//!   the last seven days" cannot be a live filter. It is as fresh as the
//!   last scan.

use std::fmt::Write as _;

use crate::model::{BASES_FOLDER, Daw};

/// The filter every production passes: a project of one of the types.
fn filters(extra: &[&str]) -> String {
    let mut out = String::from("filters:\n  and:\n    - type == \"project\"\n    - or:\n");
    for daw in Daw::ALL {
        let _ = writeln!(out, "        - projectType == \"{}\"", daw.project_type());
    }
    for line in extra {
        let _ = writeln!(out, "    - {line}");
    }
    out
}

fn order(columns: &[&str]) -> String {
    let mut out = String::from("    order:\n");
    for column in columns {
        let _ = writeln!(out, "      - {column}");
    }
    out
}

/// Every production with every property a scan or a person writes: what
/// the actions read.
#[must_use]
pub fn all() -> String {
    format!(
        "{}views:\n  - type: table\n    name: Productions\n{}",
        filters(&[]),
        order(&[
            "file.name",
            "title",
            "projectType",
            "stage",
            "client",
            "due",
            "state",
            "key",
            "tempo",
            "length",
            "tracks",
            "plugins",
            "missingMedia",
            "missingMediaCount",
            "lastSaved",
            "changedThisWeek",
            "location",
            "file",
            "files",
            "setlist",
            "show",
            "readError",
        ])
    )
}

const COLUMNS: [&str; 8] = [
    "title",
    "projectType",
    "stage",
    "client",
    "due",
    "key",
    "tempo",
    "tracks",
];

/// The default bases: each file's name in [`BASES_FOLDER`], and its YAML.
#[must_use]
pub fn defaults() -> Vec<(&'static str, String)> {
    let columns = order(&COLUMNS);
    vec![
        (
            "By stage.base",
            format!(
                "{}views:\n  - type: kanban\n    name: Board\n{columns}    columnBy: stage\n  - type: table\n    name: By stage\n    groupBy: stage\n{columns}",
                filters(&[])
            ),
        ),
        (
            "By client.base",
            format!(
                "{}views:\n  - type: table\n    name: By client\n    groupBy: client\n{columns}",
                filters(&[])
            ),
        ),
        (
            "By due date.base",
            format!(
                "{}views:\n  - type: table\n    name: By due date\n{}    sort:\n      - property: due\n        direction: ASC\n",
                filters(&[]),
                order(&["title", "due", "stage", "client", "projectType"])
            ),
        ),
        (
            "Changed this week.base",
            format!(
                "{}views:\n  - type: table\n    name: Changed this week\n{}    sort:\n      - property: lastSaved\n        direction: DESC\n",
                filters(&["changedThisWeek == true"]),
                order(&["title", "lastSaved", "stage", "client", "projectType"])
            ),
        ),
        (
            "Missing media.base",
            format!(
                "{}views:\n  - type: table\n    name: Missing media\n{}",
                filters(&["missingMediaCount > 0"]),
                order(&[
                    "title",
                    "missingMediaCount",
                    "missingMedia",
                    "stage",
                    "file"
                ])
            ),
        ),
    ]
}

/// Where default base `name` is written.
#[must_use]
pub fn path(name: &str) -> String {
    format!("{BASES_FOLDER}/{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_bases_all_yaml() {
        let bases = defaults();
        assert_eq!(bases.len(), 5);
        for (name, yaml) in bases
            .iter()
            .map(|(n, y)| (*n, y.as_str()))
            .chain([("all", all().as_str())])
        {
            let parsed: Result<serde_yaml::Value, _> = serde_yaml::from_str(yaml);
            assert!(parsed.is_ok(), "{name}: {yaml}");
            assert!(yaml.contains("projectType == \"ableton-set\""), "{name}");
        }
        assert_eq!(path("By stage.base"), "Productions/By stage.base");
    }
}
