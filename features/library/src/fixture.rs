//! A band's library, made up but shaped like a real one: two spaces, eight
//! songs (one with three arrangements over the same material), two
//! setlists with three parts attached, an album with its projects, and a
//! few conversations. The screens are designed and reviewed against it
//! until the synced docs feed them.

use uuid::Uuid;

use crate::model::*;

/// Stable ids, so a layout or a selection kept between runs still finds
/// its target.
const fn n(v: u128) -> Uuid {
    Uuid::from_u128(0x5f0d_10ad_0000_0000_0000_0000_0000_0000 | v)
}

const ME: &str = "Cody";
const BAND: SpaceId = SpaceId(n(1));
const MINE: SpaceId = SpaceId(n(2));

fn edited(when: &str, by: &str, device: &str) -> Edited {
    // "20m", "2h", "Yesterday", or a date (counted as days back from
    // Oct 26).
    let age = if let Some(m) = when.strip_suffix('m') {
        m.parse().unwrap_or(0)
    } else if let Some(h) = when.strip_suffix('h') {
        h.parse::<u32>().unwrap_or(0) * 60
    } else if when == "Yesterday" {
        24 * 60
    } else {
        let day: u32 = when
            .rsplit(' ')
            .next()
            .and_then(|d| d.parse().ok())
            .unwrap_or(1);
        let back = if when.starts_with("Sep") {
            26 + 30 - day
        } else {
            26u32.saturating_sub(day)
        };
        back * 24 * 60
    };
    Edited {
        when: when.into(),
        age,
        by: by.into(),
        device: device.into(),
    }
}

fn four(bpm: f32) -> Time {
    Time {
        bpm,
        beats_per_bar: 4,
    }
}

/// Song `i`'s ids: its sections at `n(i·1000 + 100 + k)`, its
/// arrangements at `n(i·1000 + 200 + k)`.
const fn sec(i: u128, k: u128) -> SectionId {
    SectionId(n(i * 1000 + 100 + k))
}
const fn arr(i: u128, k: u128) -> ArrangementId {
    ArrangementId(n(i * 1000 + 200 + k))
}
const fn song_id(i: u128) -> SongId {
    SongId(n(i * 1000))
}

/// The usual pop form over (intro, verse, pre, chorus, bridge, outro).
fn sections(i: u128, bars: [u32; 6]) -> Vec<Section> {
    ["Intro", "Verse", "Pre-Chorus", "Chorus", "Bridge", "Outro"]
        .iter()
        .zip(bars)
        .enumerate()
        .map(|(k, (name, bars))| Section {
            id: sec(i, k as u128),
            name: (*name).into(),
            bars,
        })
        .collect()
}

fn steps(i: u128, form: &[(u128, u8)]) -> Vec<Step> {
    form.iter()
        .map(|&(k, times)| Step {
            section: sec(i, k),
            times,
        })
        .collect()
}

const FULL: &[(u128, u8)] = &[
    (0, 1),
    (1, 1),
    (2, 1),
    (3, 1),
    (1, 1),
    (2, 1),
    (3, 2),
    (4, 2),
    (3, 2),
    (5, 1),
];

#[allow(clippy::too_many_arguments)]
fn song(
    i: u128,
    title: &str,
    colour: &'static str,
    key: &str,
    bpm: f32,
    bars: [u32; 6],
    chart: Ready,
    audio: Ready,
    edited: Edited,
    sync: Sync,
) -> Song {
    Song {
        id: song_id(i),
        space: BAND,
        title: title.into(),
        artist: "Harbor & Pine".into(),
        writers: vec!["Cody Wright".into(), "Sam Okafor".into()],
        ccli: Some(format!("7{:06}", 214_000 + i * 37)),
        tags: vec!["Original".into()],
        colour,
        sections: sections(i, bars),
        arrangements: vec![Arrangement {
            id: arr(i, 0),
            name: "Full".into(),
            key: Key(key.into()),
            time: four(bpm),
            form: steps(i, FULL),
            chart,
            audio,
            project: None,
        }],
        assets: vec![
            Asset {
                kind: AssetKind::Chart,
                name: "chart.kf".into(),
                bytes: 6_200,
                ready: chart,
            },
            Asset {
                kind: AssetKind::Stems,
                name: "stems (12)".into(),
                bytes: 612_000_000,
                ready: audio,
            },
            Asset {
                kind: AssetKind::Mix,
                name: "practice mix.wav".into(),
                bytes: 48_000_000,
                ready: audio,
            },
            Asset {
                kind: AssetKind::Click,
                name: "click + cues.wav".into(),
                bytes: 9_400_000,
                ready: audio,
            },
        ],
        edited,
        sync,
    }
}

fn entry(id: u128, song: u128, arrangement: u128) -> Entry {
    Entry {
        id: EntryId(n(id)),
        piece: Piece::Song {
            song: song_id(song),
            arrangement: arr(song, arrangement),
        },
        key: None,
        bpm: None,
        notes: String::new(),
    }
}

fn other(id: u128, kind: PieceKind, title: &str, seconds: f32) -> Entry {
    Entry {
        id: EntryId(n(id)),
        piece: Piece::Other {
            kind,
            title: title.into(),
            seconds,
        },
        key: None,
        bpm: None,
        notes: String::new(),
    }
}

/// The library.
#[must_use]
#[allow(clippy::too_many_lines)] // one literal dataset, read top to bottom
pub fn library() -> Library {
    let h = Ready::Here;
    let c = Ready::Cloud;
    let m = Ready::Missing;

    let mut songs = vec![
        song(
            1,
            "Harbor Lights",
            "#38bdf8",
            "D",
            92.0,
            [4, 8, 4, 8, 8, 4],
            h,
            h,
            edited("2h", "Sam", "MacBook Pro"),
            Sync::Offline,
        ),
        song(
            2,
            "Glass Houses",
            "#f97316",
            "Em",
            118.0,
            [4, 16, 8, 8, 8, 4],
            h,
            h,
            edited("Yesterday", ME, "iPad"),
            Sync::Offline,
        ),
        song(
            3,
            "Paper Boats",
            "#22c55e",
            "A",
            76.0,
            [4, 8, 4, 8, 8, 8],
            h,
            h,
            edited("20m", ME, "iPad"),
            Sync::Offline,
        ),
        song(
            4,
            "Long Way Home",
            "#a78bfa",
            "G",
            104.0,
            [4, 8, 4, 8, 8, 4],
            m,
            h,
            edited("Oct 21", "Jess", "iPhone"),
            Sync::Synced,
        ),
        song(
            5,
            "Static Bloom",
            "#f472b6",
            "Bm",
            126.0,
            [8, 8, 8, 8, 16, 8],
            h,
            c,
            edited("Oct 19", "Sam", "Studio Mac"),
            Sync::Cloud,
        ),
        song(
            6,
            "Late Train",
            "#facc15",
            "C",
            88.0,
            [4, 8, 4, 8, 8, 4],
            h,
            h,
            edited("Oct 18", ME, "Studio Mac"),
            Sync::Synced,
        ),
        song(
            7,
            "Open Field",
            "#2dd4bf",
            "F",
            70.0,
            [8, 8, 0, 8, 8, 8],
            h,
            h,
            edited("Oct 12", "Jess", "MacBook Air"),
            Sync::Synced,
        ),
        song(
            8,
            "Neon Orchard",
            "#fb7185",
            "E",
            132.0,
            [8, 8, 8, 8, 8, 16],
            c,
            c,
            edited("Oct 2", "Sam", "Studio Mac"),
            Sync::Syncing(0.62),
        ),
    ];

    // Paper Boats is played three ways over the same material: the record,
    // a short version for the Sunday set, and the orchestral one for the
    // Christmas concert (its own string stems beside the shared ones).
    let paper = &mut songs[2];
    paper.ccli = Some("7214111".into());
    paper.tags = vec!["Original".into(), "Single".into(), "Worship".into()];
    paper.arrangements[0].project = Some(ProjectId(n(9001)));
    paper.arrangements.push(Arrangement {
        id: arr(3, 1),
        name: "Short live".into(),
        key: Key("A".into()),
        time: four(76.0),
        form: steps(3, &[(1, 1), (3, 1), (4, 1), (3, 2), (5, 1)]),
        chart: h,
        audio: h,
        project: None,
    });
    paper.arrangements.push(Arrangement {
        id: arr(3, 2),
        name: "Orchestral".into(),
        key: Key("Bb".into()),
        time: four(72.0),
        form: steps(3, &[(0, 2), (1, 2), (2, 1), (3, 2), (4, 2), (3, 2), (5, 2)]),
        chart: h,
        audio: m,
        project: Some(ProjectId(n(9002))),
    });
    paper.assets.push(Asset {
        kind: AssetKind::Stems,
        name: "strings + brass (8)".into(),
        bytes: 402_000_000,
        ready: c,
    });
    paper.assets.push(Asset {
        kind: AssetKind::Lyrics,
        name: "lyrics.txt".into(),
        bytes: 1_800,
        ready: h,
    });
    songs[0].arrangements[0].project = Some(ProjectId(n(9003)));
    songs[1].arrangements[0].project = Some(ProjectId(n(9004)));
    songs[5].arrangements[0].project = Some(ProjectId(n(9005)));

    let tonight = Setlist {
        id: SetlistId(n(500)),
        space: BAND,
        title: "Sunday Night Worship".into(),
        when: Some("Tonight · 7:30 PM".into()),
        venue: Some("Crossroads Chapel".into()),
        entries: vec![
            other(501, PieceKind::CountIn, "Pad in A, count-in", 45.0),
            entry(502, 3, 1),
            entry(503, 1, 0),
            other(504, PieceKind::Transition, "Into Glass Houses", 20.0),
            Entry {
                key: Some(Key("D".into())),
                notes: "Capo 2, Sam starts alone".into(),
                ..entry(505, 2, 0)
            },
            entry(506, 4, 0),
            other(507, PieceKind::Spoken, "Prayer over pad", 120.0),
            Entry {
                bpm: Some(84.0),
                ..entry(508, 6, 0)
            },
            entry(509, 7, 0),
        ],
        edited: edited("20m", "Sam", "MacBook Pro"),
        sync: Sync::Offline,
        tonight: true,
    };
    let release = Setlist {
        id: SetlistId(n(600)),
        space: BAND,
        title: "Album Release Show".into(),
        when: Some("Sat, Nov 14 · 8:00 PM".into()),
        venue: Some("The Grey Eagle".into()),
        entries: vec![
            entry(601, 2, 0),
            entry(602, 1, 0),
            entry(603, 5, 0),
            entry(604, 3, 0),
            entry(605, 6, 0),
            entry(606, 8, 0),
            entry(607, 4, 0),
            entry(608, 7, 0),
        ],
        edited: edited("Oct 22", ME, "iPad"),
        sync: Sync::Synced,
        tonight: false,
    };
    let christmas = Setlist {
        id: SetlistId(n(700)),
        space: BAND,
        title: "Christmas Concert".into(),
        when: Some("Sat, Dec 19 · 6:00 PM".into()),
        venue: Some("First Baptist Hall".into()),
        entries: vec![entry(701, 3, 2), entry(702, 7, 0)],
        edited: edited("Oct 9", "Jess", "MacBook Air"),
        sync: Sync::Cloud,
        tonight: false,
    };
    let acoustic = Setlist {
        id: SetlistId(n(800)),
        space: MINE,
        title: "Coffee shop, acoustic".into(),
        when: None,
        venue: None,
        entries: vec![entry(801, 6, 0), entry(802, 3, 1), entry(803, 1, 0)],
        edited: edited("Sep 30", ME, "iPhone"),
        sync: Sync::Synced,
        tonight: false,
    };

    let guitar = Rig {
        id: RigId(n(40)),
        kind: RigKind::Guitar,
        name: "Guitar".into(),
        owner: ME.into(),
        profile: "Worship".into(),
        default_patch: "All-Around".into(),
        patches: 41,
        presets: vec![
            SongPreset {
                song: song_id(3),
                arrangement: None,
                patch: "Swell Clean".into(),
            },
            SongPreset {
                song: song_id(3),
                arrangement: Some(arr(3, 1)),
                patch: "Ambient Pad".into(),
            },
            SongPreset {
                song: song_id(1),
                arrangement: None,
                patch: "Edge Crunch".into(),
            },
            SongPreset {
                song: song_id(2),
                arrangement: None,
                patch: "Dotted Eighth".into(),
            },
            SongPreset {
                song: song_id(6),
                arrangement: None,
                patch: "Warm Clean".into(),
            },
        ],
        sync: Sync::Offline,
    };
    let keys = Rig {
        id: RigId(n(41)),
        kind: RigKind::Keys,
        name: "Keys".into(),
        owner: ME.into(),
        profile: "Worship".into(),
        default_patch: "Felt Piano".into(),
        patches: 18,
        presets: vec![
            SongPreset {
                song: song_id(3),
                arrangement: None,
                patch: "Pad + Piano".into(),
            },
            SongPreset {
                song: song_id(4),
                arrangement: None,
                patch: "Wurli".into(),
            },
        ],
        sync: Sync::Synced,
    };
    let bass = Rig {
        id: RigId(n(42)),
        kind: RigKind::Bass,
        name: "Bass".into(),
        owner: "Jess".into(),
        profile: "Live".into(),
        default_patch: "DI + Amp".into(),
        patches: 9,
        presets: vec![],
        sync: Sync::Synced,
    };

    let parts = vec![
        Part {
            id: PartId(n(50)),
            setlist: tonight.id,
            rig: guitar.id,
            player: ME.into(),
            sounds: vec![(EntryId(n(509)), "Big Lead".into())],
            sync: Sync::Offline,
        },
        Part {
            id: PartId(n(51)),
            setlist: tonight.id,
            rig: keys.id,
            player: ME.into(),
            sounds: vec![],
            sync: Sync::Synced,
        },
        Part {
            id: PartId(n(52)),
            setlist: tonight.id,
            rig: bass.id,
            player: "Jess".into(),
            sounds: vec![],
            sync: Sync::Synced,
        },
        Part {
            id: PartId(n(53)),
            setlist: release.id,
            rig: guitar.id,
            player: ME.into(),
            sounds: vec![],
            sync: Sync::Synced,
        },
    ];

    let project = |id: u128,
                   title: &str,
                   song: u128,
                   stage: Stage,
                   tracks: u32,
                   when: &str,
                   by: &str,
                   sync: Sync| Project {
        id: ProjectId(n(id)),
        space: BAND,
        title: title.into(),
        song: Some(song_id(song)),
        stage,
        tracks,
        edited: edited(when, by, "Studio Mac"),
        sync,
    };
    let projects = vec![
        project(
            9001,
            "Paper Boats",
            3,
            Stage::Mixing,
            38,
            "2h",
            "Sam",
            Sync::Syncing(0.62),
        ),
        project(
            9002,
            "Paper Boats (Orchestral)",
            3,
            Stage::Tracking,
            64,
            "Oct 20",
            ME,
            Sync::Cloud,
        ),
        project(
            9003,
            "Harbor Lights",
            1,
            Stage::Mastering,
            41,
            "Yesterday",
            "Sam",
            Sync::Synced,
        ),
        project(
            9004,
            "Glass Houses",
            2,
            Stage::Mixing,
            29,
            "Oct 22",
            ME,
            Sync::Synced,
        ),
        project(
            9005,
            "Late Train",
            6,
            Stage::Editing,
            33,
            "Oct 17",
            ME,
            Sync::Synced,
        ),
        Project {
            id: ProjectId(n(9006)),
            space: MINE,
            title: "Untitled idea, voice memo".into(),
            song: None,
            stage: Stage::Writing,
            tracks: 3,
            edited: edited("Oct 24", ME, "iPhone"),
            sync: Sync::Synced,
        },
    ];

    let album = Album {
        id: AlbumId(n(300)),
        space: BAND,
        title: "Paper Boats".into(),
        artist: "Harbor & Pine".into(),
        year: 2026,
        tracks: [
            (2, 9004),
            (1, 9003),
            (3, 9001),
            (6, 9005),
            (4, 0),
            (5, 0),
            (8, 0),
            (7, 0),
        ]
        .into_iter()
        .map(|(song, project)| Track {
            song: song_id(song),
            arrangement: arr(song, 0),
            project: (project != 0).then(|| ProjectId(n(project))),
        })
        .collect(),
        edited: edited("2h", "Sam", "MacBook Pro"),
        sync: Sync::Synced,
    };

    let say = |author: &str, when: &str, text: &str| Comment {
        author: author.into(),
        when: when.into(),
        text: text.into(),
    };
    let threads = vec![
        Thread {
            anchor: Anchor::Entry(tonight.id, EntryId(n(505))),
            comments: vec![
                say(
                    "Sam",
                    "1h",
                    "Taking Glass Houses down to D tonight, Jess's voice is tired.",
                ),
                say(
                    ME,
                    "40m",
                    "Got it, capo 2. I'll start the pad in D under the prayer.",
                ),
            ],
            resolved: false,
        },
        Thread {
            anchor: Anchor::Arrangement(song_id(3), arr(3, 1)),
            comments: vec![say(
                "Jess",
                "Yesterday",
                "Short live cuts the second verse. Can we keep the tag at the end?",
            )],
            resolved: false,
        },
        Thread {
            anchor: Anchor::Project(ProjectId(n(9001))),
            comments: vec![
                say(
                    "Sam",
                    "2h",
                    "Mix v6 is up. Vocals sit better, the kick still fights the bass at 60 Hz.",
                ),
                say(ME, "1h", "I'll print a new bass DI tonight."),
            ],
            resolved: false,
        },
        Thread {
            anchor: Anchor::Setlist(release.id),
            comments: vec![say(
                ME,
                "Oct 22",
                "Draft order. Neon Orchard might move up so we finish on Open Field.",
            )],
            resolved: false,
        },
    ];

    Library {
        me: ME.into(),
        spaces: vec![
            Space {
                id: BAND,
                name: "Harbor & Pine".into(),
                band: true,
                members: vec![ME.into(), "Sam".into(), "Jess".into(), "Dre".into()],
            },
            Space {
                id: MINE,
                name: "Personal".into(),
                band: false,
                members: vec![ME.into()],
            },
        ],
        songs,
        setlists: vec![tonight, release, christmas, acoustic],
        albums: vec![album],
        projects,
        rigs: vec![guitar, keys, bass],
        parts,
        threads,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reference_in_the_fixture_resolves() {
        let lib = library();
        for set in &lib.setlists {
            assert!(lib.space(set.space).is_some());
            for e in &set.entries {
                if let Piece::Song { song, arrangement } = e.piece {
                    let song = lib.song(song).expect("entry names a song");
                    assert!(
                        song.arrangement(arrangement).is_some(),
                        "{}: arrangement",
                        song.title
                    );
                }
            }
        }
        for part in &lib.parts {
            assert!(lib.setlist(part.setlist).is_some() && lib.rig(part.rig).is_some());
        }
        for album in &lib.albums {
            for t in &album.tracks {
                let song = lib.song(t.song).expect("track names a song");
                assert!(song.arrangement(t.arrangement).is_some());
                if let Some(p) = t.project {
                    assert!(lib.project(p).is_some());
                }
            }
        }
    }

    #[test]
    fn tonights_set_runs_about_forty_minutes() {
        let lib = library();
        let tonight = lib.setlists.iter().find(|s| s.tonight).unwrap();
        let minutes = lib.setlist_seconds(tonight) / 60.0;
        assert!((25.0..60.0).contains(&minutes), "{minutes}");
    }
}
