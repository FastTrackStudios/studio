//! A Live set that is a whole show, split into its songs.
//!
//! A recital or a gig is often one Live set: every song laid end to end on
//! one arrangement, marked the way AbleSet reads it — a locator named for
//! each song where it starts, and one named `SONG END` (or `END`, `STOP`)
//! where it ends. Locators starting with `>` are AbleSet's sections inside a
//! song, not songs. A song with no end marker runs until the next song
//! starts (the last one, until the set's end).
//!
//! [`songs`] reads that into ranges, in beats and in seconds (through the
//! set's tempo map, ramps included), each with a slug — the setlist's
//! `song:<slug>` — so the show can become a setlist of songs that share the
//! set's media.

use crate::tempo::TempoMap;
use crate::types::{AbletonLiveSet, AutomationPoint, Locator};

/// One song of a show: where it sits on the set's arrangement.
#[derive(Debug, Clone, PartialEq)]
pub struct SongRange {
    /// The song's locator name, as written (`She's American`).
    pub name: String,
    /// `shes-american`: what a setlist names it by (`song:<slug>`).
    pub slug: String,
    pub start_beats: f64,
    pub end_beats: f64,
    pub start_seconds: f64,
    pub end_seconds: f64,
}

impl SongRange {
    /// How long it plays, in seconds.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.end_seconds - self.start_seconds
    }
}

/// What a locator says about where songs start and stop.
#[derive(Debug, PartialEq, Eq)]
enum Mark<'a> {
    Start(&'a str),
    End,
    Section,
}

fn mark(name: &str) -> Mark<'_> {
    let trimmed = name.trim();
    let upper = trimmed.to_ascii_uppercase();
    if matches!(upper.as_str(), "SONG END" | "END" | "STOP" | "SONG STOP") {
        Mark::End
    } else if trimmed.starts_with('>') {
        Mark::Section
    } else {
        Mark::Start(trimmed)
    }
}

/// The songs of the show in `set`, in order (see the module docs).
#[must_use]
pub fn songs(set: &AbletonLiveSet) -> Vec<SongRange> {
    let end = set.furthest_bar * f64::from(set.time_signature.numerator.max(1));
    songs_in(&set.locators, set.tempo, &set.tempo_automation, end)
}

/// [`songs`] over the set's parts: its locators, its tempo (and tempo
/// automation, beats → bpm), and where the set ends, in beats.
#[must_use]
pub fn songs_in(
    locators: &[Locator],
    tempo: f64,
    tempo_automation: &[AutomationPoint],
    set_end_beats: f64,
) -> Vec<SongRange> {
    let mut sorted: Vec<&Locator> = locators.iter().collect();
    sorted.sort_by(|a, b| a.time.total_cmp(&b.time));
    let map = TempoMap::new(tempo, tempo_automation);
    let mut out = Vec::new();
    let mut open: Option<(&str, f64)> = None;
    let close = |out: &mut Vec<SongRange>, name: &str, start: f64, end: f64| {
        if end > start {
            out.push(SongRange {
                name: name.to_owned(),
                slug: slug(name),
                start_beats: start,
                end_beats: end,
                start_seconds: map.seconds_at(start),
                end_seconds: map.seconds_at(end),
            });
        }
    };
    for locator in sorted {
        match mark(&locator.name) {
            Mark::Section => {}
            Mark::End => {
                if let Some((name, start)) = open.take() {
                    close(&mut out, name, start, locator.time);
                }
            }
            Mark::Start(name) => {
                if let Some((open_name, start)) = open.take() {
                    close(&mut out, open_name, start, locator.time);
                }
                open = Some((name, locator.time));
            }
        }
    }
    if let Some((name, start)) = open {
        close(&mut out, name, start, set_end_beats.max(start));
    }
    out
}

/// `She's American` → `shes-american`; `HEY!` → `hey`.
#[must_use]
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().filter(|c| *c != '\'' && *c != '’') {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(time: f64, name: &str) -> Locator {
        Locator {
            time,
            name: name.to_owned(),
        }
    }

    #[test]
    fn songs_run_from_their_locator_to_their_end_marker_in_order() {
        // As JT's recital writes it: names and SONG ENDs, not in order.
        let set = [
            at(768.0, "She's American"),
            at(240.0, "Drive To LA"),
            at(0.0, "Cinematic Intro"),
            at(1264.0, "SONG END"),
            at(764.0, "SONG END"),
        ];
        let got = songs_in(&set, 120.0, &[], 2000.0);
        let names: Vec<_> = got
            .iter()
            .map(|s| (s.slug.as_str(), s.start_beats, s.end_beats))
            .collect();
        assert_eq!(
            names,
            [
                ("cinematic-intro", 0.0, 240.0),
                ("drive-to-la", 240.0, 764.0),
                ("shes-american", 768.0, 1264.0),
            ],
            "a song without an end runs to the next; the gap after an end is no song"
        );
        assert!(
            (got[1].start_seconds - 120.0).abs() < 1e-9,
            "240 beats at 120 bpm"
        );
    }

    #[test]
    fn sections_are_not_songs_and_the_last_song_runs_to_the_set_end() {
        let set = [at(0.0, "Fire"), at(16.0, ">Chorus"), at(64.0, "Know Me")];
        let got = songs_in(&set, 120.0, &[], 128.0);
        let spans: Vec<_> = got
            .iter()
            .map(|s| (s.name.as_str(), s.start_beats, s.end_beats))
            .collect();
        assert_eq!(spans, [("Fire", 0.0, 64.0), ("Know Me", 64.0, 128.0)]);
    }

    #[test]
    fn slugs_drop_apostrophes_and_punctuation() {
        assert_eq!(slug("She's American"), "shes-american");
        assert_eq!(slug("HEY!"), "hey");
        assert_eq!(slug("Heart isn't Ready"), "heart-isnt-ready");
        assert_eq!(slug("  Not Too Much Longer "), "not-too-much-longer");
    }

    /// JT's recital, the set this was written for: `ABLETON_SHOW=<path to
    /// the .als> cargo test -p ableton -- --ignored show_from_env --nocapture`.
    #[test]
    #[ignore = "reads a real Live set named by ABLETON_SHOW"]
    fn show_from_env() {
        let path = std::env::var("ABLETON_SHOW").expect("ABLETON_SHOW");
        let set = crate::io::read_live_set(&path).expect("the set");
        let got = songs(&set);
        for (i, s) in got.iter().enumerate() {
            println!(
                "{:>2}. {:<24} {:<22} beats {:>7.1}..{:<7.1} {:>6.1}s..{:<6.1}s ({:.0}s)",
                i + 1,
                s.name,
                s.slug,
                s.start_beats,
                s.end_beats,
                s.start_seconds,
                s.end_seconds,
                s.seconds()
            );
        }
        assert!(!got.is_empty(), "a show with no songs");
    }
}
