//! A setlist: its note (`type: setlist`), its page, and its edits.
//!
//! The running order is links to songs of any library (an arrangement's
//! heading, when it plays one), in named sets when it has them.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::frontmatter::{self, link_target, wikilink};
use crate::layout::SongRef;
use crate::song::Song;
use crate::{Error, Result};

pub const KIND: &str = "setlist";

/// The fence a setlist's page lists a set's songs in, one a line
/// ([`Line`]); Studio's share renderer draws it as the songs a page plays
/// (`setlists::share`), and where nothing draws it, it reads as text.
pub const SETLIST_FENCE: &str = "studio-setlist";

/// A setlist.
///
/// Its songs are in running order, in **sets** when it has them: a named
/// run of its songs ("Presidents Songs", then "Tree Lighting Concert
/// Set"). A setlist without sets is one run of songs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Setlist {
    pub title: String,
    /// When it is played (`2026-12-24`), as the leader wrote it.
    pub date: Option<String>,
    /// The songs, in running order.
    pub songs: Vec<SongRef>,
    /// Its sets, in order, covering `songs` from the first: none, or as
    /// many songs between them as `songs` has.
    pub sets: Vec<Set>,
    /// The code of its share link, once shared.
    pub share: Option<String>,
}

/// A set of a setlist: a run of its songs, named (the songs before the
/// first named set, when a set is added to a setlist that has songs, are
/// a set with no name).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Set {
    pub name: Option<String>,
    /// How many of the setlist's songs it holds.
    pub len: usize,
}

/// A set as its songs: its name and its run of the setlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section<'a> {
    pub name: Option<&'a str>,
    /// The 0-based position of its first song in the setlist.
    pub start: usize,
    pub songs: &'a [SongRef],
}

/// A set as its note writes it.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SetFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    songs: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Fields {
    #[serde(rename = "type")]
    kind: String,
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    /// The running order: every song, whatever its set.
    #[serde(default)]
    songs: Vec<String>,
    /// Its sets, each with its songs; when it has them, they are the
    /// running order (`songs` is written beside them for whoever reads
    /// only that).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sets: Vec<SetFields>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    share: Option<String>,
}

/// What a setlist's page says of a song beside its name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongInfo {
    /// `A · 83 bpm · 3/4` of the chart it plays.
    pub summary: String,
    /// Its recording, by vault path: what its page plays.
    pub recording: Option<String>,
    /// Who it is by.
    pub artist: Option<String>,
    /// How long its recording runs, in seconds ([`Song::length`]).
    pub length: Option<f64>,
}

impl SongInfo {
    /// What `at` plays of `song`: its arrangement's chart's summary, and
    /// its recording.
    #[must_use]
    pub fn of(at: &SongRef, song: &Song) -> Self {
        Self {
            summary: song
                .chart_of(at.arrangement.as_deref())
                .map(crate::song::Chart::summary)
                .unwrap_or_default(),
            recording: song
                .recording()
                .map(|r| at.library().song_file(&at.title, r)),
            artist: song.artist.clone(),
            length: song.length,
        }
    }
}

/// One song as a line of its set's fence ([`SETLIST_FENCE`]).
///
/// `<n> | <label> | <note>[#<arrangement>] | <summary> | <recording> |
/// <artist> | <length>`, its note and recording by vault path, its length
/// in seconds; the last four possibly empty (a line written before the
/// last two were has five fields). A `|` or `\` in a field is written
/// `\|`, `\\`.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// Its 1-based place in the running order.
    pub number: usize,
    pub label: String,
    /// Its note (`Libraries/<L>/<S>/<S>.md`).
    pub note: String,
    /// The arrangement it plays: a heading of its note.
    pub arrangement: Option<String>,
    pub summary: String,
    pub recording: Option<String>,
    pub artist: Option<String>,
    /// Seconds.
    pub length: Option<f64>,
}

impl Line {
    fn write(&self) -> String {
        let note = self
            .arrangement
            .as_ref()
            .map_or_else(|| self.note.clone(), |a| format!("{}#{a}", self.note));
        let mut fields = vec![
            self.number.to_string(),
            self.label.clone(),
            note,
            self.summary.clone(),
            self.recording.clone().unwrap_or_default(),
            self.artist.clone().unwrap_or_default(),
            self.length.map(|l| format!("{l:.1}")).unwrap_or_default(),
        ];
        while fields.last().is_some_and(String::is_empty) {
            fields.pop();
        }
        fields
            .iter()
            .map(|f| f.replace('\\', "\\\\").replace('|', "\\|"))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    /// `line`'s fields: split at each `|` not escaped (`\|`), each
    /// unescaped and trimmed.
    fn fields(line: &str) -> Vec<String> {
        let mut fields = Vec::new();
        let mut field = String::new();
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => field.push(chars.next().unwrap_or('\\')),
                '|' => fields.push(std::mem::take(&mut field).trim().to_owned()),
                c => field.push(c),
            }
        }
        fields.push(field.trim().to_owned());
        fields
    }

    /// A line of the fence; None for one that is not a song's.
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let fields = Self::fields(line);
        let mut parts = fields.iter().map(String::as_str);
        let number = parts.next()?.parse().ok()?;
        let label = parts.next()?.to_owned();
        let target = parts.next()?;
        let (note, arrangement) = match target.split_once('#') {
            Some((n, a)) => (n.to_owned(), Some(a.to_owned()).filter(|a| !a.is_empty())),
            None => (target.to_owned(), None),
        };
        if note.is_empty() || label.is_empty() {
            return None;
        }
        let summary = parts.next().unwrap_or_default().to_owned();
        let recording = parts.next().filter(|r| !r.is_empty()).map(str::to_owned);
        let artist = parts.next().filter(|a| !a.is_empty()).map(str::to_owned);
        let length = parts
            .next()
            .and_then(|l| l.parse::<f64>().ok())
            .filter(|l| l.is_finite() && *l > 0.0);
        Some(Self {
            number,
            label,
            note,
            arrangement,
            summary,
            recording,
            artist,
            length,
        })
    }
}

impl Setlist {
    /// A setlist of `songs`, no sets.
    #[must_use]
    pub fn new(title: &str, songs: Vec<SongRef>) -> Self {
        Self {
            title: title.to_owned(),
            songs,
            ..Self::default()
        }
    }

    /// Its sets as runs of its songs, in order; one with no name for a
    /// setlist without sets.
    #[must_use]
    pub fn sections(&self) -> Vec<Section<'_>> {
        if self.sets.is_empty() {
            return vec![Section {
                name: None,
                start: 0,
                songs: &self.songs,
            }];
        }
        let mut start = 0_usize;
        self.sets
            .iter()
            .map(|set| {
                let end = start.saturating_add(set.len).min(self.songs.len());
                let section = Section {
                    name: set.name.as_deref(),
                    start,
                    songs: self.songs.get(start..end).unwrap_or_default(),
                };
                start = end;
                section
            })
            .collect()
    }

    /// The set named `name` (any case): its index.
    fn set_named(&self, name: &str) -> Option<usize> {
        self.sets.iter().position(|s| {
            s.name
                .as_deref()
                .is_some_and(|n| n.trim().eq_ignore_ascii_case(name.trim()))
        })
    }

    /// Where set `index` starts in the running order.
    fn set_start(&self, index: usize) -> usize {
        self.sets
            .iter()
            .take(index)
            .fold(0_usize, |n, s| n.saturating_add(s.len))
    }

    /// The set that song position `index` (0-based) falls in: the one
    /// holding it, or the last for the place past the end.
    fn set_at(&self, index: usize) -> Option<usize> {
        let mut end = 0_usize;
        for (i, set) in self.sets.iter().enumerate() {
            end = end.saturating_add(set.len);
            if index < end {
                return Some(i);
            }
        }
        self.sets.len().checked_sub(1)
    }

    /// Set `name`, made at the end when it has none (the songs it has
    /// already, outside any set, become a set with no name first).
    fn set_or_new(&mut self, name: &str) -> Result<usize> {
        crate::check_name(name)?;
        if let Some(i) = self.set_named(name) {
            return Ok(i);
        }
        if self.sets.is_empty() && !self.songs.is_empty() {
            self.sets.push(Set {
                name: None,
                len: self.songs.len(),
            });
        }
        self.sets.push(Set {
            name: Some(name.trim().to_owned()),
            len: 0,
        });
        Ok(self.sets.len().saturating_sub(1))
    }

    /// Insert `song` at 0-based `index` of the running order, in set
    /// `set` (None: the set that place is in).
    fn insert(&mut self, index: usize, song: SongRef, set: Option<usize>) {
        let set = set.or_else(|| self.set_at(index));
        self.songs.insert(index, song);
        if let Some(s) = set.and_then(|i| self.sets.get_mut(i)) {
            s.len = s.len.saturating_add(1);
        }
    }

    /// Sets with no songs dropped; a lone set with no name is no set.
    fn tidy(&mut self) {
        self.sets.retain(|s| s.len > 0);
        if let [only] = self.sets.as_slice() {
            if only.name.is_none() {
                self.sets.clear();
            }
        }
    }

    /// Its note. `known` are the songs as their libraries have them (a
    /// song it does not find is still listed, by name).
    #[must_use]
    pub fn note(&self, known: &[(SongRef, Song)]) -> String {
        frontmatter::write(
            &self.fields(),
            &self.page(|at| {
                known
                    .iter()
                    .find(|(r, _)| r.library == at.library && r.title == at.title)
                    .map(|(_, song)| SongInfo::of(at, song))
            }),
        )
    }

    /// Its properties as its note writes them.
    fn fields(&self) -> Fields {
        let links = |songs: &[SongRef]| -> Vec<String> {
            songs
                .iter()
                .map(|s| wikilink(&s.link_target(), &s.label()))
                .collect()
        };
        Fields {
            kind: KIND.to_owned(),
            title: self.title.clone(),
            date: self.date.clone(),
            songs: links(&self.songs),
            sets: if self.sets.is_empty() {
                Vec::new()
            } else {
                self.sections()
                    .iter()
                    .map(|s| SetFields {
                        name: s.name.map(str::to_owned),
                        songs: links(s.songs),
                    })
                    .collect()
            },
            share: self.share.clone(),
        }
    }

    /// Its `songs` property: a wikilink per song, in running order.
    #[must_use]
    pub fn songs_property(&self) -> serde_json::Value {
        serde_json::to_value(self.fields().songs).unwrap_or_default()
    }

    /// Its `sets` property (None: it has no sets).
    #[must_use]
    pub fn sets_property(&self) -> Option<serde_json::Value> {
        let fields = self.fields();
        (!fields.sets.is_empty()).then(|| serde_json::to_value(fields.sets).unwrap_or_default())
    }

    /// Its page: its title and date, then each set (a heading for a named
    /// one) and its songs as a [`SETLIST_FENCE`], each song with what
    /// `info` says of it; the last set's fence asks for the link's
    /// downloads (`downloads` in its info).
    #[must_use]
    pub fn page(&self, info: impl Fn(&SongRef) -> Option<SongInfo>) -> String {
        let mut out = format!("# {}\n\n", self.title);
        if let Some(date) = &self.date {
            let _ = writeln!(out, "*{date}*\n");
        }
        if self.songs.is_empty() {
            out.push_str("No songs yet.\n");
            return out;
        }
        let sections = self.sections();
        let last = sections.len().saturating_sub(1);
        for (n, section) in sections.iter().enumerate() {
            if let Some(name) = section.name {
                let _ = writeln!(out, "## {name}\n");
            }
            // The last set's block ends with the link's downloads.
            let downloads = if n == last { " downloads" } else { "" };
            let _ = writeln!(out, "```{SETLIST_FENCE}{downloads}");
            for (i, at) in section.songs.iter().enumerate() {
                let info = info(at).unwrap_or_default();
                let line = Line {
                    number: section.start.saturating_add(i).saturating_add(1),
                    label: at.label(),
                    note: at.library().song_note(&at.title),
                    arrangement: at.arrangement.clone(),
                    summary: info.summary,
                    recording: info.recording,
                    artist: info.artist,
                    length: info.length,
                };
                let _ = writeln!(out, "{}", line.write());
            }
            out.push_str("```\n\n");
        }
        out
    }

    /// The setlist a note describes; None when it is not one. A song link
    /// that is not to a library's song is dropped.
    #[must_use]
    pub fn from_note(text: &str) -> Option<Self> {
        let f: Fields = frontmatter::read(text)?;
        if f.kind != KIND {
            return None;
        }
        let refs = |links: &[String]| -> Vec<SongRef> {
            links
                .iter()
                .filter_map(|l| SongRef::from_link_target(link_target(l)))
                .collect()
        };
        let mut set = Self {
            title: f.title,
            date: f.date,
            songs: refs(&f.songs),
            sets: Vec::new(),
            share: f.share,
        };
        if !f.sets.is_empty() {
            set.songs.clear();
            for s in &f.sets {
                let songs = refs(&s.songs);
                set.sets.push(Set {
                    name: s.name.clone().filter(|n| !n.trim().is_empty()),
                    len: songs.len(),
                });
                set.songs.extend(songs);
            }
            set.tidy();
        }
        Some(set)
    }

    /// Add `song` at `at` (1-based; None: at the end), of set `set` when
    /// one is named (made at the end if it has none), else of the whole
    /// running order (in the set that place is in).
    ///
    /// # Errors
    ///
    /// [`Error::Position`] when `at` is past the end; [`Error::BadName`]
    /// for a set name that is not one.
    pub fn add(&mut self, song: SongRef, at: Option<usize>, set: Option<&str>) -> Result<()> {
        if let Some(name) = set {
            let set = self.set_or_new(name)?;
            let start = self.set_start(set);
            let len = self.sets.get(set).map_or(0, |s| s.len);
            let index = match at {
                None => len,
                Some(at) => index_of(at, len.saturating_add(1))?,
            };
            self.insert(start.saturating_add(index), song, Some(set));
        } else {
            let len = self.songs.len();
            let index = match at {
                None => len,
                Some(at) => index_of(at, len.saturating_add(1))?,
            };
            self.insert(index, song, None);
        }
        self.tidy();
        Ok(())
    }

    /// Take out `song`: its 1-based position, or the first whose title or
    /// label (`Song (Arrangement)`) it is, any case.
    ///
    /// # Errors
    ///
    /// [`Error::NoSong`] when it is not in the setlist.
    pub fn remove(&mut self, song: &str) -> Result<SongRef> {
        let index = self.position(song)?;
        let entry = self.take(index);
        self.tidy();
        Ok(entry)
    }

    /// Take out the song at 0-based `index`, its set one shorter (an
    /// emptied set stays until [`Self::tidy`], so a move keeps its place).
    fn take(&mut self, index: usize) -> SongRef {
        if let Some(s) = self.set_at(index).and_then(|i| self.sets.get_mut(i)) {
            s.len = s.len.saturating_sub(1);
        }
        self.songs.remove(index)
    }

    /// Move `song` (as [`Self::remove`] finds it) to `to` (1-based): of
    /// set `set` when one is named (made at the end if it has none), else
    /// of the whole running order, joining the set it lands in.
    ///
    /// # Errors
    ///
    /// When it is not in the setlist, `to` is outside it, or `set` is not
    /// a usable name.
    pub fn move_song(&mut self, song: &str, to: usize, set: Option<&str>) -> Result<()> {
        let from = self.position(song)?;
        if let Some(name) = set {
            crate::check_name(name)?;
            let target = self.set_named(name);
            let room = target
                .and_then(|i| self.sets.get(i))
                .map_or(0, |s| s.len)
                .saturating_add(usize::from(target.is_none() || self.set_at(from) != target));
            index_of(to, room.max(1))?;
            let entry = self.take(from);
            return self.add(entry, Some(to), Some(name));
        }
        let to = index_of(to, self.songs.len())?;
        let entry = self.take(from);
        self.insert(to, entry, None);
        self.tidy();
        Ok(())
    }

    fn position(&self, song: &str) -> Result<usize> {
        let song = song.trim();
        if let Ok(n) = song.parse::<usize>() {
            return index_of(n, self.songs.len());
        }
        self.songs
            .iter()
            .position(|s| s.label().eq_ignore_ascii_case(song))
            .or_else(|| {
                self.songs
                    .iter()
                    .position(|s| s.title.eq_ignore_ascii_case(song))
            })
            .ok_or_else(|| Error::NoSong(song.to_owned()))
    }
}

/// The index of 1-based position `at` in a list of `len`.
const fn index_of(at: usize, len: usize) -> Result<usize> {
    if at == 0 || at > len {
        Err(Error::Position { at, len })
    } else {
        Ok(at.saturating_sub(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> Setlist {
        Setlist::new(
            "Christmas Eve",
            vec![
                SongRef::new("Christmas", "Joy to the World", None),
                SongRef::new("Hymns", "Silent Night", Some("Acoustic")),
            ],
        )
    }

    #[test]
    fn a_setlist_note_reads_back_as_the_setlist() {
        let mut s = set();
        s.date = Some("2026-12-24".to_owned());
        s.share = Some("abcdefghijkmnpqr".to_owned());
        let note = s.note(&[]);
        assert!(
            note.contains(
                "```studio-setlist downloads\n1 | Joy to the World | Libraries/Christmas/Joy to the World/Joy to the World.md\n"
            ),
            "{note}"
        );
        assert!(
            note.contains(
                "2 | Silent Night (Acoustic) | Libraries/Hymns/Silent Night/Silent Night.md#Acoustic\n```"
            ),
            "{note}"
        );
        assert!(!note.contains("sets:"), "no sets, no property: {note}");
        assert_eq!(Setlist::from_note(&note), Some(s));
    }

    #[test]
    fn edits_keep_the_order() {
        let mut s = set();
        s.add(
            SongRef::new("Christmas", "Sleigh Ride", None),
            Some(1),
            None,
        )
        .ok();
        let titles = |s: &Setlist| s.songs.iter().map(SongRef::label).collect::<Vec<_>>();
        assert_eq!(
            titles(&s),
            ["Sleigh Ride", "Joy to the World", "Silent Night (Acoustic)"]
        );
        s.move_song("silent night", 1, None).ok();
        assert_eq!(
            titles(&s),
            ["Silent Night (Acoustic)", "Sleigh Ride", "Joy to the World"]
        );
        assert_eq!(
            s.remove("2").ok().map(|r| r.title).as_deref(),
            Some("Sleigh Ride")
        );
        assert!(matches!(
            s.add(SongRef::new("X", "Y", None), Some(9), None),
            Err(Error::Position { at: 9, len: 3 })
        ));
        assert!(matches!(s.remove("Nope"), Err(Error::NoSong(_))));
    }

    /// The owner's concert: two named sets, played through in order.
    fn concert() -> Setlist {
        let song = |t: &str| SongRef::new("Christmas", t, None);
        let mut s = Setlist::new("Tree Lighting", Vec::new());
        for t in ["Hail to the Chief", "America", "Stars and Stripes"] {
            s.add(song(t), None, Some("Presidents Songs")).unwrap();
        }
        for t in ["Joy to the World", "Silent Night", "Jingle Bells"] {
            s.add(song(t), None, Some("Tree Lighting Concert Set"))
                .unwrap();
        }
        s
    }

    #[test]
    fn sets_are_runs_of_the_running_order() {
        let s = concert();
        let names = |s: &Setlist| {
            s.sections()
                .iter()
                .map(|x| (x.name.map(str::to_owned), x.start, x.songs.len()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(&s),
            [
                (Some("Presidents Songs".to_owned()), 0, 3),
                (Some("Tree Lighting Concert Set".to_owned()), 3, 3)
            ]
        );
        let note = s.note(&[]);
        assert!(
            note.contains("sets:\n- name: Presidents Songs\n  songs:\n"),
            "{note}"
        );
        assert!(note.contains("## Tree Lighting Concert Set\n\n```studio-setlist downloads\n4 | Joy to the World |"), "{note}");
        assert_eq!(Setlist::from_note(&note), Some(s.clone()));

        // A move in the running order joins the set it lands in; a move
        // into a set by name lands at that place of it.
        let mut moved = s;
        moved.move_song("Jingle Bells", 1, None).unwrap();
        assert_eq!(
            names(&moved)[0],
            (Some("Presidents Songs".to_owned()), 0, 4)
        );
        moved
            .move_song("America", 1, Some("Tree Lighting Concert Set"))
            .unwrap();
        let tree = moved.sections()[1];
        assert_eq!(tree.songs[0].title, "America");
        assert_eq!(tree.songs.len(), 3);

        // A set's only song moved within it: the set keeps its place.
        let mut solo = Setlist::new("Solo", Vec::new());
        solo.add(SongRef::new("C", "A", None), None, Some("First"))
            .unwrap();
        solo.add(SongRef::new("C", "B", None), None, Some("Second"))
            .unwrap();
        solo.move_song("A", 1, Some("First")).unwrap();
        assert_eq!(
            solo.sections()
                .iter()
                .map(|x| x.name.unwrap_or_default())
                .collect::<Vec<_>>(),
            ["First", "Second"]
        );

        // A set emptied goes; a lone set with no name is no set.
        let mut plain = Setlist::new("Eve", vec![SongRef::new("C", "A", None)]);
        plain
            .add(SongRef::new("C", "B", None), None, Some("Encore"))
            .unwrap();
        assert_eq!(
            plain.sets.len(),
            2,
            "the songs before it are a set with no name"
        );
        plain.remove("B").unwrap();
        assert_eq!(plain.sets.len(), 0);
        assert!(matches!(
            plain.add(SongRef::new("C", "D", None), None, Some("a/b")),
            Err(Error::BadName(_))
        ));
    }

    #[test]
    fn page_lines_read_back() {
        let line = Line {
            number: 4,
            label: "Silent Night (Acoustic)".to_owned(),
            note: "Libraries/Hymns/Silent Night/Silent Night.md".to_owned(),
            arrangement: Some("Acoustic".to_owned()),
            summary: "G · 72 bpm · 3/4".to_owned(),
            recording: Some("Libraries/Hymns/Silent Night/Media/Silent Night.ogg".to_owned()),
            artist: Some("Franz Gruber".to_owned()),
            length: Some(186.5),
        };
        assert_eq!(Line::parse(&line.write()), Some(line));
        // A `|` or a `\` in a field stays in its field.
        let piped = Line {
            number: 1,
            label: "Medley | Joy \\ Noel".to_owned(),
            note: "Libraries/H/Medley/Medley.md".to_owned(),
            arrangement: None,
            summary: "D · 100 bpm · 2/4".to_owned(),
            recording: Some("Libraries/H/Medley/Media/Medley.mp4".to_owned()),
            artist: Some("Simon | Garfunkel".to_owned()),
            length: Some(201.5),
        };
        assert_eq!(Line::parse(&piped.write()), Some(piped));
        // A line written before the artist and the length were.
        let old = Line::parse(
            "2 | Joy | Libraries/H/Joy/Joy.md | D · 100 bpm · 2/4 | Libraries/H/Joy/Media/Joy.wav",
        )
        .unwrap();
        assert_eq!((old.artist, old.length), (None, None));
        assert_eq!(
            old.recording.as_deref(),
            Some("Libraries/H/Joy/Media/Joy.wav")
        );
        assert_eq!(Line::parse("not a song"), None);
    }
}
