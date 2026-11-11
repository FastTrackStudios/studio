//! An Ableton set that is a show, as a setlist.
//!
//! Its songs (`ableton::setlist::songs`) become notes in the shape
//! `setlists` keeps, built with that crate's public API: a song each in
//! the library named for the set (`Libraries/<Set>/<Song>/<Song>.md`,
//! `type: song`, its tempo), and a setlist (`Setlists/<Set>.md`, `type:
//! setlist`) of them in running order, with a `project` link back to the
//! production. So the setlists extension lists and opens it like any
//! other, while the show stays in Ableton. Pure: the notes' text, for
//! the host to write.

use serde_json::{Map, Value, json};
use setlists::frontmatter::{split, wikilink};
use setlists::layout::{Library, SongRef, setlist_note};
use setlists::song::{Chart, Song};
use setlists::{Setlist, check_name};

use crate::model::file_name;
use crate::read::ShowSong;

/// A note to write: where, its properties, its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub path: String,
    pub properties: Map<String, Value>,
    pub body: String,
}

/// A show's notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Show {
    /// A note per song, in running order (a song played twice, once).
    pub songs: Vec<Note>,
    pub setlist: Note,
    /// The setlist's `songs` property: links in running order.
    pub links: Value,
}

/// `text` (a note with YAML frontmatter) as a note at `path`.
fn note(path: String, text: &str) -> Note {
    let (yaml, body) = split(text);
    let properties = serde_yaml::from_str::<Value>(yaml)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    Note {
        path,
        properties,
        body: body.to_owned(),
    }
}

/// A wikilink to the note at `path` (`Projects/X/project.md`), shown as
/// `label`.
#[must_use]
pub fn link_to(path: &str, label: &str) -> String {
    wikilink(path.strip_suffix(".md").unwrap_or(path), label)
}

/// The notes of the show `title` (the production's), its `songs` in
/// order, linked back to the production's note at `project`.
#[must_use]
pub fn notes(title: &str, songs: &[ShowSong], project: &str) -> Show {
    let title = file_name(title);
    let library = Library::named(&title);
    let mut known: Vec<(SongRef, Song)> = Vec::new();
    let mut refs: Vec<SongRef> = Vec::new();
    for song in songs {
        let name = file_name(&song.name);
        if check_name(&name).is_err() {
            continue;
        }
        let at = SongRef::new(&library.name, &name, None);
        refs.push(at.clone());
        if known.iter().any(|(r, _)| *r == at) {
            continue;
        }
        let tempo = (song.tempo * 100.0).round() / 100.0;
        known.push((
            at,
            Song {
                title: name,
                chart: Chart {
                    tempo: (tempo > 0.0).then_some(tempo),
                    ..Chart::default()
                },
                ..Song::default()
            },
        ));
    }
    let set = Setlist::new(&title, refs);
    let mut setlist = note(setlist_note(&title), &set.note(&known));
    setlist
        .properties
        .insert("project".to_owned(), json!(link_to(project, &title)));
    let links = setlist
        .properties
        .get("songs")
        .cloned()
        .unwrap_or_else(|| json!([]));
    Show {
        songs: known
            .iter()
            .map(|(at, song)| {
                note(
                    library.song_note(&at.title),
                    &song.note(&library, &setlists::song::SongPage::default()),
                )
            })
            .collect(),
        setlist,
        links,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    fn song(name: &str, tempo: f64) -> ShowSong {
        ShowSong {
            name: name.to_owned(),
            tempo,
            seconds: 200.0,
        }
    }

    #[test]
    fn a_show_reads_back_as_setlists_setlist() {
        let show = notes(
            "Spring Tour",
            &[
                song("She's American", 120.0),
                song("Robbers", 92.5),
                song("She's American", 120.0),
            ],
            "Projects/Spring Tour/project.md",
        );
        assert_eq!(show.songs.len(), 2);
        assert_eq!(
            show.songs[0].path,
            "Libraries/Spring Tour/She's American/She's American.md"
        );
        assert_eq!(show.songs[0].properties["type"], "song");
        assert_eq!(show.songs[1].properties["tempo"], json!(92.5));
        assert_eq!(show.setlist.path, "Setlists/Spring Tour.md");
        assert_eq!(
            show.setlist.properties["project"],
            "[[Projects/Spring Tour/project|Spring Tour]]"
        );
        assert_eq!(show.links.as_array().unwrap().len(), 3);

        // The same text setlists writes: it reads back as its Setlist.
        let text = format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&show.setlist.properties).unwrap(),
            show.setlist.body
        );
        let read = Setlist::from_note(&text).unwrap();
        assert_eq!(read.title, "Spring Tour");
        let titles: Vec<&str> = read.songs.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["She's American", "Robbers", "She's American"]);
        assert!(read.songs.iter().all(|s| s.library == "Spring Tour"));
        let lib = Library::named("Spring Tour");
        let first = format!(
            "---\n{}---\n{}",
            serde_yaml::to_string(&show.songs[0].properties).unwrap(),
            show.songs[0].body
        );
        assert_eq!(
            Song::from_note(&lib, &first).unwrap().chart.tempo,
            Some(120.0)
        );
    }
}
