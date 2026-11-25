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

/// A section card: a clip on the set's sections track naming the stretch
/// of the song it spans (`Verse 1`, `Chorus`, `Count In`) — AbleSet's
/// cards, a MIDI clip with no notes, named and coloured.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub name: String,
    pub start_beats: f64,
    pub end_beats: f64,
    /// The card's colour, `0xRRGGBB` (Live's palette).
    pub color: Option<u32>,
}

/// The set's section cards, in order: the arrangement clips of every MIDI
/// track whose name starts with `SECTION` (`SECTIONS +CC`), disabled and
/// unnamed ones left out.
#[must_use]
pub fn sections(set: &AbletonLiveSet) -> Vec<Section> {
    let mut out: Vec<Section> = set
        .midi_tracks
        .iter()
        .filter(|t| {
            t.common
                .effective_name
                .trim()
                .to_ascii_uppercase()
                .starts_with("SECTION")
        })
        .flat_map(|t| &t.arrangement_clips)
        .map(|c| &c.common)
        .filter(|c| !c.disabled && !c.name.trim().is_empty() && c.current_end > c.current_start)
        .map(|c| Section {
            name: c.name.trim().to_owned(),
            start_beats: c.current_start,
            end_beats: c.current_end,
            color: live_color(c.color),
        })
        .collect();
    out.sort_by(|a, b| a.start_beats.total_cmp(&b.start_beats));
    out
}

/// The cards of `song`: those starting inside it, cut at its end.
#[must_use]
pub fn sections_of(sections: &[Section], song: &SongRange) -> Vec<Section> {
    sections
        .iter()
        .filter(|s| s.start_beats >= song.start_beats - 1e-9 && s.start_beats < song.end_beats)
        .map(|s| Section {
            end_beats: s.end_beats.min(song.end_beats),
            ..s.clone()
        })
        .collect()
}

/// Whether a card is the count-in before the song (`Count In`,
/// `COUNT-IN`): not a section, the bars before bar 1.
#[must_use]
pub fn is_count_in(name: &str) -> bool {
    name.to_ascii_uppercase().replace(['-', ' ', '_'], "") == "COUNTIN"
}

/// Live's clip and track palette (indices 0–69, five rows of 14), as
/// `0xRRGGBB`; `None` past it.
#[must_use]
pub fn live_color(index: i32) -> Option<u32> {
    const PALETTE: [u32; 70] = [
        0xFF94A6, 0xFFA529, 0xCC9927, 0xF7F47C, 0xBFFB00, 0x1AFF2F, 0x25FFA8, 0x5CFFE8, 0x8BC5FF,
        0x5480E4, 0x92A7FF, 0xD86CE4, 0xE553A0, 0xFFFFFF, //
        0xFF3636, 0xF66C03, 0x99724B, 0xFFF034, 0x87FF67, 0x3DC300, 0x00BFAF, 0x19E9FF, 0x10A4EE,
        0x007DC0, 0x886CE4, 0xB677C6, 0xFF39D4, 0xD0D0D0, //
        0xE2675A, 0xFFA374, 0xD3AD71, 0xEDFFAE, 0xD2E498, 0xBAD074, 0x9BC48D, 0xD4FDE1, 0xCDF1F8,
        0xB9C1E3, 0xCDBBE4, 0xAE98E5, 0xE5DCE1, 0xA9A9A9, //
        0xC6928B, 0xB78256, 0x99836A, 0xBFBA69, 0xA6BE00, 0x7DB04D, 0x88C2BA, 0x9BB3C4, 0x85A5C2,
        0x8393CC, 0xA595B5, 0xBF9FBE, 0xBC7196, 0x7B7B7B, //
        0xAF3333, 0xA95131, 0x724F41, 0xDBC300, 0x85961F, 0x539F31, 0x0A9C8E, 0x236384, 0x1A2F96,
        0x2F52A2, 0x624BAD, 0xA34BAD, 0xCC2E6E, 0x3C3C3C,
    ];
    usize::try_from(index)
        .ok()
        .and_then(|i| PALETTE.get(i).copied())
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

    /// JT's recital's sections track, cut down: a count-in, two cards of
    /// She's American, one of the next song, a disabled card, and a clip
    /// on another track that is not a card.
    const CARDS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Ableton MajorVersion="5" MinorVersion="12.0_12049" Creator="Ableton Live 12.2.5">
  <LiveSet><Tracks>
    <MidiTrack Id="1"><Name><EffectiveName Value="SECTIONS +CC" /><UserName Value="SECTIONS +CC" /></Name>
      <DeviceChain><MainSequencer><ClipTimeable><ArrangerAutomation><Events>
        <MidiClip Id="2" Time="776"><CurrentStart Value="776" /><CurrentEnd Value="808" /><Name Value="Intro" /><Color Value="14" /><Disabled Value="false" /></MidiClip>
        <MidiClip Id="1" Time="768"><CurrentStart Value="768" /><CurrentEnd Value="776" /><Name Value="Count In" /><Color Value="65" /><Disabled Value="false" /></MidiClip>
        <MidiClip Id="3" Time="808"><CurrentStart Value="808" /><CurrentEnd Value="1300" /><Name Value=" Verse 1 " /><Color Value="20" /><Disabled Value="false" /></MidiClip>
        <MidiClip Id="4" Time="1272"><CurrentStart Value="1272" /><CurrentEnd Value="1280" /><Name Value="Count In" /><Color Value="65" /><Disabled Value="false" /></MidiClip>
        <MidiClip Id="5" Time="900"><CurrentStart Value="900" /><CurrentEnd Value="904" /><Name Value="Muted" /><Color Value="1" /><Disabled Value="true" /></MidiClip>
      </Events></ArrangerAutomation></ClipTimeable></MainSequencer></DeviceChain>
    </MidiTrack>
    <MidiTrack Id="2"><Name><EffectiveName Value="CLICK" /><UserName Value="CLICK" /></Name>
      <DeviceChain><MainSequencer><ClipTimeable><ArrangerAutomation><Events>
        <MidiClip Id="6" Time="768"><CurrentStart Value="768" /><CurrentEnd Value="800" /><Name Value="Click" /><Color Value="1" /></MidiClip>
      </Events></ArrangerAutomation></ClipTimeable></MainSequencer></DeviceChain>
    </MidiTrack>
  </Tracks></LiveSet>
</Ableton>"#;

    #[test]
    fn a_songs_sections_are_the_cards_on_the_sections_track_inside_it() {
        let set = crate::parse::parse_live_set(CARDS).expect("parses");
        let all = sections(&set);
        let names: Vec<_> = all.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["Count In", "Intro", "Verse 1", "Count In"],
            "in order, trimmed, cards only"
        );
        assert_eq!(all[1].color, Some(0xFF3636), "Live's colour 14");
        let song = SongRange {
            name: "She's American".into(),
            slug: "shes-american".into(),
            start_beats: 768.0,
            end_beats: 1264.0,
            start_seconds: 288.0,
            end_seconds: 551.0,
        };
        let cards: Vec<_> = sections_of(&all, &song)
            .into_iter()
            .map(|s| (s.name, s.start_beats, s.end_beats))
            .collect();
        assert_eq!(
            cards,
            [
                ("Count In".to_owned(), 768.0, 776.0),
                ("Intro".to_owned(), 776.0, 808.0),
                ("Verse 1".to_owned(), 808.0, 1264.0),
            ],
            "the next song's count-in is not this song's; a card runs to the song's end at most"
        );
        assert!(is_count_in("Count In") && is_count_in("COUNT-IN") && !is_count_in("Intro"));
        assert_eq!(live_color(70), None);
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
