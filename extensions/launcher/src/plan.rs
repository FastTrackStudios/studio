//! What a setlist's note keeps about the set itself, written through the
//! vault: a line of notes per song, the rigs it is played with, the
//! layout it opens in, and each song's session.
//!
//! All of it is the note's frontmatter (the owner's decision: these are
//! about this set and must sync to the band), so an edit here syncs and
//! undoes like any edit of the note.

use serde_json::{Map, Value, json};

use crate::model::Part;
use crate::{Found, host};

/// Sets what setlist `found` says of song `label` (empty: says nothing).
///
/// # Errors
///
/// The song is not in the setlist, or the vault refused.
pub async fn note(found: &Found, label: &str, text: &str) -> Result<Value, String> {
    let list = &found.setlist;
    let entry = list
        .entries()
        .find(|e| e.label.eq_ignore_ascii_case(label.trim()))
        .ok_or_else(|| format!("no song `{label}` in {}", list.title))?;
    let mut notes: Map<String, Value> = list
        .entries()
        .filter_map(|e| Some((e.label.clone(), Value::String(e.remark.clone()?))))
        .collect();
    let text = text.trim();
    if text.is_empty() {
        notes.remove(&entry.label);
    } else {
        notes.insert(entry.label.clone(), Value::String(text.to_owned()));
    }
    let value = (!notes.is_empty()).then_some(Value::Object(notes));
    host::set_property(&list.path, "notes", value.as_ref()).await?;
    Ok(json!({ "setlist": list.title, "song": entry.label, "note": text }))
}

/// What to do to a setlist's parts.
pub struct PartEdit<'a> {
    pub player: &'a str,
    pub rig: &'a str,
    pub profile: Option<&'a str>,
    /// A song and the patch it takes (empty: the profile's default).
    pub patch: Option<(&'a str, &'a str)>,
    /// Take the part out.
    pub remove: bool,
}

/// `parts` after `edit`: the part of that player and rig made, changed
/// or taken out.
#[must_use]
pub fn edited(mut parts: Vec<Part>, edit: &PartEdit<'_>) -> Vec<Part> {
    let is = |p: &Part| {
        p.player.eq_ignore_ascii_case(edit.player) && p.rig.eq_ignore_ascii_case(edit.rig)
    };
    if edit.remove {
        parts.retain(|p| !is(p));
        return parts;
    }
    if !parts.iter().any(is) {
        parts.push(Part {
            player: edit.player.to_owned(),
            rig: edit.rig.to_owned(),
            ..Part::default()
        });
    }
    for part in parts.iter_mut().filter(|p| is(p)) {
        if let Some(profile) = edit.profile {
            part.profile = Some(profile.to_owned());
        }
        if let Some((song, patch)) = edit.patch {
            part.patches.retain(|(s, _)| !s.eq_ignore_ascii_case(song));
            if !patch.trim().is_empty() {
                part.patches
                    .push((song.to_owned(), patch.trim().to_owned()));
            }
        }
    }
    parts
}

/// Makes, changes or takes out a rig setlist `found` is played with.
///
/// # Errors
///
/// The vault refused.
pub async fn part(found: &Found, edit: &PartEdit<'_>) -> Result<Value, String> {
    let list = &found.setlist;
    let parts = edited(list.parts.clone(), edit);
    let value = (!parts.is_empty()).then(|| Value::Array(parts.iter().map(Part::json).collect()));
    host::set_property(&list.path, "parts", value.as_ref()).await?;
    Ok(json!({ "setlist": list.title, "parts": parts.iter().map(Part::json).collect::<Vec<_>>() }))
}

/// Remembers `layout` as the one setlist `found` opens in.
///
/// # Errors
///
/// The vault refused.
pub async fn layout(found: &Found, layout: &str) -> Result<(), String> {
    if found.setlist.layout.as_deref() == Some(layout) {
        return Ok(());
    }
    host::set_property(
        &found.setlist.path,
        "layout",
        Some(&Value::String(layout.to_owned())),
    )
    .await
}

/// Writes into each song's note the session found for it by name, as a
/// path in the project root, so the note names it from then on.
///
/// # Errors
///
/// The vault has no folder on disk.
pub async fn name_sessions(found: &Found, vault_root: &std::path::Path) -> Result<Value, String> {
    let project = crate::find::project_root(vault_root, &found.setlist.project);
    let mut named = Vec::new();
    let mut left = Vec::new();
    for ((entry, session), by_note) in found
        .setlist
        .entries()
        .zip(&found.sessions)
        .zip(&found.named)
    {
        let (Some(song), Some(session), false) = (&entry.song, session, *by_note) else {
            continue;
        };
        let Some(written) = crate::find::in_project(session, &project) else {
            left.push(json!({ "song": entry.label, "why": "its session is outside the project" }));
            continue;
        };
        match host::set_property(&song.path, "session", Some(&Value::String(written.clone()))).await
        {
            Ok(()) => named.push(json!({ "song": entry.label, "session": written })),
            Err(why) => left.push(json!({ "song": entry.label, "why": why })),
        }
    }
    Ok(json!({ "setlist": found.setlist.title, "named": named, "left": left }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit<'a>(player: &'a str, rig: &'a str) -> PartEdit<'a> {
        PartEdit {
            player,
            rig,
            profile: None,
            patch: None,
            remove: false,
        }
    }

    #[test]
    fn a_part_is_made_changed_and_taken_out() {
        let parts = edited(
            Vec::new(),
            &PartEdit {
                profile: Some("worship"),
                ..edit("Cody", "guitar")
            },
        );
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].profile.as_deref(), Some("worship"));
        let parts = edited(
            parts,
            &PartEdit {
                patch: Some(("Drive To LA", "clean")),
                ..edit("cody", "Guitar")
            },
        );
        assert_eq!(parts.len(), 1);
        assert_eq!(
            parts[0].patches,
            [("Drive To LA".to_owned(), "clean".to_owned())]
        );
        let parts = edited(
            parts,
            &PartEdit {
                patch: Some(("drive to la", "")),
                ..edit("Cody", "guitar")
            },
        );
        assert_eq!(parts[0].patches, []);
        let parts = edited(parts, &edit("JT", "keys"));
        assert_eq!(parts.len(), 2);
        let parts = edited(
            parts,
            &PartEdit {
                remove: true,
                ..edit("Cody", "guitar")
            },
        );
        assert_eq!(
            parts.iter().map(|p| p.rig.as_str()).collect::<Vec<_>>(),
            ["keys"]
        );
    }
}
