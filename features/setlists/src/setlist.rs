//! A setlist: its note (`type: setlist`), the running order as links to
//! songs of any library (an arrangement's heading, when it plays one),
//! and the edits a leader makes to it.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::frontmatter::{self, link_target, wikilink};
use crate::layout::SongRef;
use crate::song::Song;
use crate::{Error, Result};

pub const KIND: &str = "setlist";

/// A setlist.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Setlist {
    pub title: String,
    /// When it is played (`2026-12-24`), as the leader wrote it.
    pub date: Option<String>,
    /// The songs, in running order.
    pub songs: Vec<SongRef>,
    /// The code of its share link, once shared.
    pub share: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Fields {
    #[serde(rename = "type")]
    kind: String,
    title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    date: Option<String>,
    #[serde(default)]
    songs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    share: Option<String>,
}

impl Setlist {
    #[must_use]
    pub fn new(title: &str, songs: Vec<SongRef>) -> Self {
        Self {
            title: title.to_owned(),
            songs,
            ..Self::default()
        }
    }

    /// Its note. `known` are the songs as their libraries have them (a
    /// song it does not find is still listed, by name).
    #[must_use]
    pub fn note(&self, known: &[(SongRef, Song)]) -> String {
        let fields = Fields {
            kind: KIND.to_owned(),
            title: self.title.clone(),
            date: self.date.clone(),
            songs: self
                .songs
                .iter()
                .map(|s| wikilink(&s.link_target(), &s.label()))
                .collect(),
            share: self.share.clone(),
        };
        frontmatter::write(&fields, &self.body(known))
    }

    fn body(&self, known: &[(SongRef, Song)]) -> String {
        let mut out = format!("# {}\n\n", self.title);
        if let Some(date) = &self.date {
            let _ = writeln!(out, "*{date}*\n");
        }
        if self.songs.is_empty() {
            out.push_str("No songs yet.\n");
            return out;
        }
        for (n, entry) in (1_u32..).zip(&self.songs) {
            let link = wikilink(&entry.link_target(), &entry.label());
            let summary = known
                .iter()
                .find(|(r, _)| r.library == entry.library && r.title == entry.title)
                .and_then(|(_, song)| song.chart_of(entry.arrangement.as_deref()))
                .map(crate::song::Chart::summary)
                .filter(|s| !s.is_empty());
            let from = &entry.library;
            match summary {
                Some(summary) => {
                    let _ = writeln!(out, "{n}. {link} — {summary} · *{from}*");
                }
                None => {
                    let _ = writeln!(out, "{n}. {link} · *{from}*");
                }
            }
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
        Some(Self {
            title: f.title,
            date: f.date,
            songs: f
                .songs
                .iter()
                .filter_map(|l| SongRef::from_link_target(link_target(l)))
                .collect(),
            share: f.share,
        })
    }

    /// Add `song` at `at` (1-based; None: at the end).
    ///
    /// # Errors
    ///
    /// [`Error::Position`] when `at` is past the end.
    pub fn add(&mut self, song: SongRef, at: Option<usize>) -> Result<()> {
        let len = self.songs.len();
        let index = match at {
            None => len,
            Some(at) => index_of(at, len.saturating_add(1))?,
        };
        self.songs.insert(index, song);
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
        Ok(self.songs.remove(index))
    }

    /// Move `song` (as [`Self::remove`] finds it) to `to` (1-based).
    ///
    /// # Errors
    ///
    /// When it is not in the setlist or `to` is outside it.
    pub fn move_song(&mut self, song: &str, to: usize) -> Result<()> {
        let from = self.position(song)?;
        let to = index_of(to, self.songs.len())?;
        let entry = self.songs.remove(from);
        self.songs.insert(to, entry);
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
        assert!(note.contains(
            "1. [[Libraries/Christmas/Joy to the World/Joy to the World|Joy to the World]] · *Christmas*"
        ), "{note}");
        assert!(
            note.contains(
                "2. [[Libraries/Hymns/Silent Night/Silent Night#Acoustic|Silent Night (Acoustic)]]"
            ),
            "{note}"
        );
        assert_eq!(Setlist::from_note(&note), Some(s));
    }

    #[test]
    fn edits_keep_the_order() {
        let mut s = set();
        s.add(SongRef::new("Christmas", "Sleigh Ride", None), Some(1))
            .ok();
        let titles = |s: &Setlist| s.songs.iter().map(SongRef::label).collect::<Vec<_>>();
        assert_eq!(
            titles(&s),
            ["Sleigh Ride", "Joy to the World", "Silent Night (Acoustic)"]
        );
        s.move_song("silent night", 1).ok();
        assert_eq!(
            titles(&s),
            ["Silent Night (Acoustic)", "Sleigh Ride", "Joy to the World"]
        );
        assert_eq!(
            s.remove("2").ok().map(|r| r.title).as_deref(),
            Some("Sleigh Ride")
        );
        assert!(matches!(
            s.add(SongRef::new("X", "Y", None), Some(9)),
            Err(Error::Position { at: 9, len: 3 })
        ));
        assert!(matches!(s.remove("Nope"), Err(Error::NoSong(_))));
    }
}
