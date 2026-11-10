//! The two-stage sidebar: navigator providers (the MIT
//! `architect-navigator-api`; this crate never links the GPL navigator).
//!
//! - **Setlists** (`STUDIO_SETLISTS_NAV_*`): the setlists on the left;
//!   the songs of the one selected on the right, in running order.
//! - **Libraries** (`STUDIO_SETLISTS_LIBRARIES_NAV_*`): each library, its
//!   categories under it; a category's songs on the right (a song in two
//!   categories is in both), a library's uncategorised songs under it.
//!
//! A song opens as a scope tab (`Open::Context`, type `song`): its tab
//! says its key and tempo, and the "Song" screenset lays it out.

use architect_navigator_api::{Item, NavigatorProvider, Node, Open, ProviderFuture, Snapshot};

use crate::host::{SetlistRow, SongRow};
use crate::layout::SongRef;

/// The setlists provider's action prefix.
pub const SETLISTS: &str = "STUDIO_SETLISTS_NAV";
/// The libraries provider's action prefix.
pub const LIBRARIES: &str = "STUDIO_SETLISTS_LIBRARIES_NAV";

/// The context type a song loads as.
pub const SONG: &str = "song";

/// How song `at` opens: a scope tab of its own.
#[must_use]
pub fn open_song(at: &SongRef, song: Option<&SongRow>) -> Open {
    let mut properties: Vec<(String, String)> = Vec::new();
    if let Some(s) = song {
        if let Some(k) = &s.key {
            properties.push(("key".to_owned(), k.clone()));
        }
        if let Some(t) = &s.tempo {
            properties.push(("tempo".to_owned(), t.clone()));
        }
    }
    properties.push(("library".to_owned(), at.library.clone()));
    if let Some(a) = &at.arrangement {
        properties.push(("arrangement".to_owned(), a.clone()));
    }
    Open::Context {
        context_type: SONG.to_owned(),
        id: format!("{}.md", at.library().song_link_target(&at.title)),
        title: at.label(),
        folder: Some(at.library().song_dir(&at.title)),
        properties,
    }
}

fn song_item(id: String, container: &str, at: &SongRef, song: Option<&SongRow>) -> Item {
    let mut item = Item::new(id, container, at.label()).opens(open_song(at, song));
    item.icon = Some("music".to_owned());
    let summary = song.map(SongRow::summary).unwrap_or_default();
    let preview = if summary.is_empty() {
        at.library.clone()
    } else {
        format!("{summary} · {}", at.library)
    };
    item = item.preview(preview);
    if let Some(s) = song {
        for (k, v) in [("key", &s.key), ("tempo", &s.tempo), ("meter", &s.meter)] {
            if let Some(v) = v {
                item = item.property(k, v.clone());
            }
        }
        for c in &s.categories {
            item = item.tag(format!("category/{c}"));
        }
    }
    item
}

/// The setlists snapshot, from the rows read.
#[must_use]
pub fn setlists_snapshot(sets: &[SetlistRow], songs: &[SongRow]) -> Snapshot {
    let mut snap = Snapshot::new("Setlists").ordered();
    for set in sets {
        let mut node = Node::new(set.path.clone(), set.title.clone()).icon("list-music");
        if set.share.is_some() {
            node.shared = true;
        }
        snap = snap.node(node);
        for (n, at) in (1_u32..).zip(&set.songs) {
            let known = songs
                .iter()
                .find(|s| s.at.library == at.library && s.at.title == at.title);
            snap = snap.item(song_item(format!("{}|{n}", set.path), &set.path, at, known));
        }
    }
    snap
}

/// The libraries snapshot, from the rows read.
#[must_use]
pub fn libraries_snapshot(songs: &[SongRow]) -> Snapshot {
    let mut libraries: Vec<&str> = songs.iter().map(|s| s.at.library.as_str()).collect();
    libraries.sort_unstable();
    libraries.dedup();
    let mut snap = Snapshot::new("Libraries");
    for lib in libraries {
        let lib_id = format!("library:{lib}");
        let mut categories: Vec<&str> = songs
            .iter()
            .filter(|s| s.at.library == lib)
            .flat_map(|s| s.categories.iter().map(String::as_str))
            .collect();
        categories.sort_unstable_by_key(|c| c.to_lowercase());
        categories.dedup_by_key(|c| c.to_lowercase());
        let mut node = Node::new(lib_id.clone(), lib).icon("library");
        for c in &categories {
            node = node.child(Node::new(format!("{lib_id}/{c}"), *c).icon("folder-music"));
        }
        snap = snap.node(node);
        for s in songs.iter().filter(|s| s.at.library == lib) {
            if s.categories.is_empty() {
                snap = snap.item(song_item(s.path.clone(), &lib_id, &s.at, Some(s)));
            }
            for c in &s.categories {
                let container = format!("{lib_id}/{c}");
                snap = snap.item(song_item(
                    format!("{container}|{}", s.path),
                    &container,
                    &s.at,
                    Some(s),
                ));
            }
        }
    }
    snap
}

/// The setlists, as the navigator shows them.
#[derive(Debug, Clone, Copy, Default)]
pub struct SetlistsNav;

impl NavigatorProvider for SetlistsNav {
    fn snapshot(&self) -> ProviderFuture<Snapshot> {
        Box::pin(async move {
            let sets = crate::host::setlists().await?;
            let songs = crate::host::songs().await.unwrap_or_default();
            Ok(setlists_snapshot(&sets, &songs))
        })
    }
}

/// The libraries, as the navigator shows them.
#[derive(Debug, Clone, Copy, Default)]
pub struct LibrariesNav;

impl NavigatorProvider for LibrariesNav {
    fn snapshot(&self) -> ProviderFuture<Snapshot> {
        Box::pin(async move { Ok(libraries_snapshot(&crate::host::songs().await?)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(lib: &str, title: &str, cats: &[&str]) -> SongRow {
        SongRow {
            at: SongRef::new(lib, title, None),
            path: format!("Libraries/{lib}/{title}/{title}.md"),
            key: Some("A".to_owned()),
            tempo: Some("83".to_owned()),
            meter: None,
            artist: None,
            categories: cats.iter().map(|c| (*c).to_owned()).collect(),
            arrangements: Vec::new(),
        }
    }

    #[test]
    fn a_setlist_shows_its_songs_in_order() {
        let songs = [
            song("Christmas", "Silent Night", &[]),
            song("Hymns", "Old Hundredth", &[]),
        ];
        let sets = [SetlistRow {
            path: "Setlists/Eve.md".to_owned(),
            title: "Eve".to_owned(),
            date: None,
            songs: vec![
                SongRef::new("Hymns", "Old Hundredth", None),
                SongRef::new("Christmas", "Silent Night", Some("Acoustic")),
            ],
            share: Some("abc".to_owned()),
        }];
        let snap = setlists_snapshot(&sets, &songs);
        assert!(snap.ordered);
        assert!(snap.tree[0].shared);
        let titles: Vec<&str> = snap.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["Old Hundredth", "Silent Night (Acoustic)"]);
        assert_eq!(snap.items[0].preview.as_deref(), Some("A · 83 bpm · Hymns"));
        match &snap.items[1].open {
            Some(Open::Context {
                context_type,
                id,
                properties,
                ..
            }) => {
                assert_eq!(context_type, SONG);
                assert_eq!(id, "Libraries/Christmas/Silent Night/Silent Night.md");
                assert!(properties.contains(&("arrangement".to_owned(), "Acoustic".to_owned())));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn libraries_show_their_categories() {
        let songs = [
            song("Christmas", "Silent Night", &["Carols", "Quiet"]),
            song("Christmas", "Sleigh Ride", &[]),
            song("Hymns", "Old Hundredth", &["Doxology"]),
        ];
        let snap = libraries_snapshot(&songs);
        assert_eq!(snap.tree.len(), 2);
        let kids: Vec<&str> = snap.tree[0]
            .children
            .iter()
            .map(|n| n.title.as_str())
            .collect();
        assert_eq!(kids, ["Carols", "Quiet"]);
        // Silent Night is in both categories; Sleigh Ride under the library.
        let in_container = |c: &str| snap.items.iter().filter(|i| i.container == c).count();
        assert_eq!(in_container("library:Christmas/Carols"), 1);
        assert_eq!(in_container("library:Christmas/Quiet"), 1);
        assert_eq!(in_container("library:Christmas"), 1);
    }
}
