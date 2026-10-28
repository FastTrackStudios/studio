//! The library as an account's Task orgs have it: each org a space, its
//! setlists (Task `setlist`/`songlist` collections) in order, and the songs
//! they hold. Plain values in (what the app read from Task), a [`Library`]
//! out; no I/O here.
//!
//! What Task does not keep yet (arrangements' forms, sections, assets,
//! parts, sessions as docs) is left empty rather than made up: a song has
//! one arrangement, in its key, with no form.

use uuid::Uuid;

use crate::model::{
    Arrangement, ArrangementId, Edited, Entry, EntryId, Key, Library, Piece, Ready, Setlist,
    SetlistId, Song, SongId, Space, SpaceId, Sync, Time,
};

/// One org of the account, and its lists.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TaskOrg {
    /// Its slug (`fasttrackaudio`).
    pub slug: String,
    pub lists: Vec<TaskList>,
}

/// One list of an org, in order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TaskList {
    /// Task's collection id.
    pub id: String,
    pub title: String,
    /// A set (a show, a service) rather than a song list.
    pub set: bool,
    pub songs: Vec<TaskSong>,
}

/// One song of a list.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TaskSong {
    pub slug: String,
    pub title: String,
    pub writers: Vec<String>,
    pub key: String,
}

/// The id a Task thing (`what`, e.g. `org/setlist/<id>`) has in the
/// library: the same every time it is read, so a page open on it stays
/// open when the library is read again.
fn id_of(what: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("task:{what}").as_bytes())
}

/// An org's slug as a person reads it: `days-to-praise` → Days To Praise.
#[must_use]
pub fn org_name(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A song's title; a song Task has no doc for comes back titled by its
/// slug (`set-in-stone`), which reads as words (`Set In Stone`).
fn title_of(song: &TaskSong) -> String {
    if song.title.is_empty() || song.title == song.slug {
        org_name(&song.slug)
    } else {
        song.title.clone()
    }
}

/// The space an org is in the library.
#[must_use]
pub fn space_of(slug: &str) -> SpaceId {
    SpaceId(id_of(slug))
}

/// The library `me` reads across `orgs`.
#[must_use]
pub fn library(me: &str, orgs: &[TaskOrg]) -> Library {
    let mut lib = Library {
        me: me.to_owned(),
        ..Library::default()
    };
    let synced = || Edited {
        when: String::new(),
        age: u32::MAX,
        by: String::new(),
        device: String::new(),
    };
    for org in orgs {
        let space = space_of(&org.slug);
        lib.spaces.push(Space {
            id: space,
            name: org_name(&org.slug),
            band: true,
            members: Vec::new(),
        });
        for list in &org.lists {
            let mut entries = Vec::new();
            for (at, song) in list.songs.iter().enumerate() {
                let song_id = SongId(id_of(&format!("{}/song/{}", org.slug, song.slug)));
                let arrangement =
                    ArrangementId(id_of(&format!("{}/song/{}/a", org.slug, song.slug)));
                if !lib.songs.iter().any(|s| s.id == song_id) {
                    lib.songs.push(Song {
                        id: song_id,
                        space,
                        title: title_of(song),
                        artist: org_name(&org.slug),
                        writers: song.writers.clone(),
                        ccli: None,
                        tags: Vec::new(),
                        colour: "#3b82f6",
                        sections: Vec::new(),
                        arrangements: vec![Arrangement {
                            id: arrangement,
                            name: "Original".to_owned(),
                            key: Key(song.key.clone()),
                            time: Time {
                                bpm: 0.0,
                                beats_per_bar: 4,
                            },
                            form: Vec::new(),
                            chart: Ready::Cloud,
                            audio: Ready::Cloud,
                            session: None,
                        }],
                        assets: Vec::new(),
                        edited: synced(),
                        sync: Sync::Synced,
                    });
                }
                entries.push(Entry {
                    id: EntryId(id_of(&format!("{}/setlist/{}/{at}", org.slug, list.id))),
                    piece: Piece::Song {
                        song: song_id,
                        arrangement,
                    },
                    key: None,
                    bpm: None,
                    notes: String::new(),
                });
            }
            lib.setlists.push(Setlist {
                id: SetlistId(id_of(&format!("{}/setlist/{}", org.slug, list.id))),
                space,
                title: list.title.clone(),
                when: None,
                venue: None,
                entries,
                edited: synced(),
                sync: Sync::Synced,
                tonight: false,
            });
        }
    }
    lib
}

#[cfg(test)]
mod tests {
    use super::{TaskList, TaskOrg, TaskSong, library, org_name, space_of};

    fn song(slug: &str, title: &str) -> TaskSong {
        TaskSong {
            slug: slug.into(),
            title: title.into(),
            writers: Vec::new(),
            key: "E".into(),
        }
    }

    fn orgs() -> Vec<TaskOrg> {
        vec![
            TaskOrg {
                slug: "fasttrackaudio".into(),
                lists: vec![
                    TaskList {
                        id: "c1".into(),
                        title: "Crescendum".into(),
                        set: true,
                        songs: vec![song("intro", "Intro"), song("unbreakable", "Unbreakable")],
                    },
                    TaskList {
                        id: "c2".into(),
                        title: "Favourites".into(),
                        set: false,
                        songs: vec![song("unbreakable", "Unbreakable")],
                    },
                ],
            },
            TaskOrg {
                slug: "tombrooksmusic".into(),
                lists: vec![TaskList {
                    id: "p1".into(),
                    title: "PNG Worship Collective".into(),
                    set: true,
                    songs: vec![song("unity", "Unity")],
                }],
            },
        ]
    }

    #[test]
    fn every_org_is_a_space_with_its_lists_and_songs() {
        let lib = library("Cody", &orgs());
        assert_eq!(lib.me, "Cody");
        assert_eq!(lib.spaces.len(), 2);
        assert_eq!(lib.spaces[1].name, "Tombrooksmusic");
        let titles: Vec<&str> = lib.setlists.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Crescendum", "Favourites", "PNG Worship Collective"]
        );
        // A song in two lists of one org is one song.
        assert_eq!(lib.songs.len(), 3);
        let png = &lib.setlists[2];
        assert_eq!(png.space, space_of("tombrooksmusic"));
        assert_eq!(png.entries.len(), 1);
    }

    #[test]
    fn reading_it_again_gives_the_same_ids() {
        assert_eq!(library("Cody", &orgs()), library("Cody", &orgs()));
    }

    #[test]
    fn an_orgs_name_is_its_words() {
        assert_eq!(org_name("days-to-praise"), "Days To Praise");
    }

    #[test]
    fn a_song_titled_by_its_slug_reads_as_words() {
        let lib = library(
            "Cody",
            &[TaskOrg {
                slug: "o".into(),
                lists: vec![TaskList {
                    id: "l".into(),
                    title: "L".into(),
                    set: true,
                    songs: vec![song("set-in-stone", "set-in-stone")],
                }],
            }],
        );
        assert_eq!(lib.songs[0].title, "Set In Stone");
    }
}
