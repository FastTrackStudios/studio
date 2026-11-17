//! Which sessions a note enters: pure, so it is tested without a vault.
//!
//! A setlist note lists its songs (`songs:`, wiki-links in running
//! order); a song note says where its session is (`session:`). The DAW
//! opens those sessions as one set through a `.setlist` file (one absolute
//! path per line, `session_engine::setlist::read_setlist`).

use std::path::{Path, PathBuf};

use serde_json::Value;

/// What a note is, for entering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entered {
    /// A setlist: its title and its songs' note paths, in running order.
    Setlist { title: String, songs: Vec<String> },
    /// One song: its note path.
    Song(String),
}

/// What `properties` (a note's frontmatter, as `VAULT_READ` answers it)
/// make of the note at `path`: a setlist (`type: setlist`), else a song.
#[must_use]
pub fn entered(path: &str, properties: &Value) -> Entered {
    if properties.get("type").and_then(Value::as_str) == Some("setlist") {
        let songs = properties
            .get("songs")
            .and_then(Value::as_array)
            .map(|songs| {
                songs
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(song_note)
                    .collect()
            })
            .unwrap_or_default();
        let title = properties
            .get("title")
            .and_then(Value::as_str)
            .map_or_else(|| stem(path), str::to_owned);
        return Entered::Setlist { title, songs };
    }
    Entered::Song(path.to_owned())
}

/// A setlist row's song note: `[[Libraries/L/S/S#Arrangement|S]]` is
/// `Libraries/L/S/S.md` (the DAW plays the song's session; an
/// arrangement's own session is later work).
#[must_use]
pub fn song_note(link: &str) -> Option<String> {
    let target = setlists::frontmatter::link_target(link);
    let target = target.split('#').next().unwrap_or(target).trim();
    if target.is_empty() {
        return None;
    }
    Some(if has_extension(target, "md") {
        target.to_owned()
    } else {
        format!("{target}.md")
    })
}

/// Where a song's `session` property points: an absolute path; `~/…`
/// under `home`; a wiki-link into the vault (`[[Libraries/…/S.session]]`,
/// against `vault`); else a path relative to the song note's folder.
#[must_use]
pub fn session_path(value: &str, note: &str, vault: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let value = value.trim().trim_matches(['"', '\'']);
    if value.is_empty() {
        return None;
    }
    if value.starts_with("[[") || value.starts_with("![[") {
        let target = setlists::frontmatter::link_target(value);
        return Some(vault.join(target));
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return home.map(|h| h.join(rest));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Some(path.to_path_buf());
    }
    let folder = Path::new(note).parent().unwrap_or_else(|| Path::new(""));
    Some(vault.join(folder).join(path))
}

/// The `.setlist` text for `sessions`: one absolute path per line.
#[must_use]
pub fn setlist_text(title: &str, sessions: &[PathBuf]) -> String {
    let mut out = format!("# {title}: entered from Architect (studio-daw)\n");
    for s in sessions {
        out.push_str(&s.display().to_string());
        out.push('\n');
    }
    out
}

/// A file name for `title`: no separators, no leading dot.
#[must_use]
pub fn file_name(title: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| if c == '/' || c == '\\' || c.is_control() { '-' } else { c })
        .collect();
    let clean = clean.trim().trim_start_matches('.').to_owned();
    if clean.is_empty() { "set".to_owned() } else { clean }
}

fn stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map_or_else(|| path.to_owned(), |s| s.to_string_lossy().into_owned())
}

fn has_extension(path: &str, ext: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_setlist_note_lists_its_songs_notes_in_order() {
        let props = json!({
            "type": "setlist",
            "title": "JT's Senior Recital",
            "songs": [
                "[[Libraries/JT/Fire/Fire|Fire]]",
                "[[Libraries/JT/Know Me/Know Me#Extended|Know Me (Extended)]]",
            ],
        });
        assert_eq!(
            entered("Setlists/JT's Senior Recital.md", &props),
            Entered::Setlist {
                title: "JT's Senior Recital".into(),
                songs: vec!["Libraries/JT/Fire/Fire.md".into(), "Libraries/JT/Know Me/Know Me.md".into()],
            }
        );
    }

    #[test]
    fn any_other_note_is_a_song() {
        let props = json!({ "type": "song", "title": "Fire" });
        assert_eq!(entered("Libraries/JT/Fire/Fire.md", &props), Entered::Song("Libraries/JT/Fire/Fire.md".into()));
    }

    #[test]
    fn a_session_is_absolute_home_linked_or_beside_the_note() {
        let vault = Path::new("/v");
        let home = Some(Path::new("/home/me"));
        let note = "Libraries/JT/Fire/Fire.md";
        assert_eq!(session_path("/s/fire/Fire.session", note, vault, home), Some("/s/fire/Fire.session".into()));
        assert_eq!(session_path("'~/songs/Fire.session'", note, vault, home), Some("/home/me/songs/Fire.session".into()));
        assert_eq!(
            session_path("[[Libraries/JT/Fire/Fire.session|Fire.session]]", note, vault, home),
            Some("/v/Libraries/JT/Fire/Fire.session".into())
        );
        assert_eq!(
            session_path("Session/Fire.session", note, vault, home),
            Some("/v/Libraries/JT/Fire/Session/Fire.session".into())
        );
        assert_eq!(session_path("  ", note, vault, home), None);
    }

    #[test]
    fn the_setlist_file_is_one_absolute_path_a_line() {
        let text = setlist_text("Set", &["/a/A.session".into(), "/b/B.session".into()]);
        assert_eq!(text.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>(), ["/a/A.session", "/b/B.session"]);
        assert_eq!(file_name("A/B"), "A-B");
        assert_eq!(file_name(".."), "set");
    }
}
