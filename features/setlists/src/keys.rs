//! What a setlist's row says of a song's key: its key's name, as its chart
//! spells it (`E`, `Eb`, `F#m`), and what is less plain about it.
//!
//! - `E→F`: the chart starts in E and ends in F (its last key in force).
//! - `E*`: it starts and ends in E but is not plainly in E: a key change
//!   that comes back, or notes outside the key past a small threshold
//!   ([`OUTSIDE`]: two different chords outside it; a secondary dominant,
//!   a major chord resolving up a fourth to a chord of the key, is not
//!   counted, so a lone `V/V` earns nothing).
//! - `E`: plainly E.
//!
//! The facts come from the chart's own parse (Keyflow's initial key,
//! ending key, key changes and each chord's root resolved in the key in
//! force there; [`facts`]), worked out when the song is brought in and
//! kept on its note (`key`, `key_end`, `key_outside`); the setlist's row
//! carries them. A setlist that plays a song in another key has every key
//! of the label moved to it ([`KeyLabel::in_key`]).

/// How many different chords outside the key earn the asterisk.
pub const OUTSIDE: usize = 2;

/// Whether key `key` is a minor key (`Em`, `F#m`; not `Emaj`).
fn minor(key: &str) -> bool {
    pitch(key).is_some_and(|(_, rest)| rest.starts_with('m') && !rest.starts_with("maj"))
}

/// The step from pitch class `from` to `to`, the shorter way: −5…+6 (a
/// tritone is +6), 0 for the same pitch however it is spelled.
#[must_use]
pub const fn semitones(from: i32, to: i32) -> i32 {
    let up = to.wrapping_sub(from).rem_euclid(12);
    if up > 6 { up.wrapping_sub(12) } else { up }
}

/// What a setlist's key for a song does to its label ([`KeyLabel::played`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Played {
    /// Its own key, or the same pitch spelled another way (`Gb` for `F#`):
    /// as charted, no mark.
    AsCharted,
    /// Transposed: the label in the played key (every key of it moved the
    /// same way), and by how far, the shorter way (−5…+6, never 0).
    Moved { label: KeyLabel, semitones: i32 },
    /// Another mode (`E` played `Em`): not a transposition. The played key
    /// alone, no mark and no number; its chart is still the song's own.
    OtherMode(KeyLabel),
}

/// A song's key as a row shows it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyLabel {
    /// The key it starts in (`E`, `F#m`).
    pub start: String,
    /// The key it ends in, when that is another.
    pub end: Option<String>,
    /// Not plainly in its key: a key change that comes back, or notes
    /// outside the key past [`OUTSIDE`].
    pub outside: bool,
}

/// The pitch class of a note name (`C` 0 … `B` 11), and the rest of it.
fn pitch(name: &str) -> Option<(i32, &str)> {
    let mut chars = name.char_indices();
    let (_, letter) = chars.next()?;
    let base: i32 = match letter.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut pc = base;
    let mut rest = chars.as_str();
    loop {
        if let Some(r) = rest.strip_prefix('#').or_else(|| rest.strip_prefix('♯')) {
            pc = pc.wrapping_add(1);
            rest = r;
        } else if let Some(r) = rest.strip_prefix('b').or_else(|| rest.strip_prefix('♭')) {
            pc = pc.wrapping_sub(1);
            rest = r;
        } else {
            break;
        }
    }
    Some((pc.rem_euclid(12), rest))
}

/// Pitch class `pc` named with sharps or flats.
fn name_of(pc: i32, flats: bool) -> &'static str {
    const SHARPS: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    const FLATS: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
    let i = usize::try_from(pc.rem_euclid(12)).unwrap_or(0);
    let names = if flats { &FLATS } else { &SHARPS };
    names.get(i).copied().unwrap_or("C")
}

/// Whether key `key` is written with flats (`Eb`, `F`, `Dm`).
fn flat_key(key: &str) -> bool {
    let Some((pc, rest)) = pitch(key) else {
        return false;
    };
    if key
        .get(1..)
        .is_some_and(|r| r.starts_with('b') || r.starts_with('♭'))
    {
        return true;
    }
    if key
        .get(1..)
        .is_some_and(|r| r.starts_with('#') || r.starts_with('♯'))
    {
        return false;
    }
    let minor = rest.starts_with('m');
    // Naturals: F major; D, G, C, F minor.
    if minor {
        [2, 7, 0, 5].contains(&pc)
    } else {
        pc == 5
    }
}

impl KeyLabel {
    /// Plainly in `key`.
    #[must_use]
    pub fn plain(key: &str) -> Self {
        Self {
            start: key.to_owned(),
            end: None,
            outside: false,
        }
    }

    /// As a row writes it: `E`, `E*`, `E→F` (a start and an end take the
    /// place of the asterisk).
    #[must_use]
    pub fn text(&self) -> String {
        match &self.end {
            Some(end) => format!("{}→{end}", self.start),
            None if self.outside => format!("{}*", self.start),
            None => self.start.clone(),
        }
    }

    /// Read back from a row (`E→F`, `E*`, `E`); None for nothing.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        if let Some((start, end)) = text.split_once('→') {
            return Some(Self {
                start: start.trim().to_owned(),
                end: Some(end.trim().to_owned()),
                outside: false,
            });
        }
        let outside = text.ends_with('*');
        Some(Self {
            start: text.trim_end_matches('*').to_owned(),
            end: None,
            outside,
        })
    }

    /// Said in words, for a screen reader: "Key of E", "Key of E, with
    /// notes outside the key", "Starts in E, ends in F".
    #[must_use]
    pub fn spoken(&self) -> String {
        let said = |k: &str| {
            let (root, minor) = k.strip_suffix('m').map_or((k, false), |r| (r, true));
            let root = root.replace('#', " sharp").replace('b', " flat");
            if minor { format!("{root} minor") } else { root }
        };
        match &self.end {
            Some(end) => format!("Starts in {}, ends in {}", said(&self.start), said(end)),
            None if self.outside => {
                format!("Key of {}, with notes outside the key", said(&self.start))
            }
            None => format!("Key of {}", said(&self.start)),
        }
    }

    /// Played in `key` (a setlist's key for the song): every key moved by
    /// the same step, spelled as `key` is. None when `key` is not a key or
    /// is this one.
    #[must_use]
    pub fn in_key(&self, key: &str) -> Option<Self> {
        let (to, _) = pitch(key)?;
        let (from, _) = pitch(&self.start)?;
        if to == from {
            return None;
        }
        let step = to.wrapping_sub(from);
        let flats = flat_key(key);
        let moved = |k: &str| -> Option<String> {
            let (pc, rest) = pitch(k)?;
            Some(format!("{}{rest}", name_of(pc.wrapping_add(step), flats)))
        };
        // Its start in the setlist's own spelling (and mode, as written).
        let (_, mode) = pitch(&self.start)?;
        let start = format!("{}{mode}", key.trim_end_matches('m'));
        Some(Self {
            start,
            end: self.end.as_deref().and_then(moved),
            outside: self.outside,
        })
    }

    /// Played in the setlist's key `key`: as charted, moved by a number of
    /// semitones (its tonic's, the shorter way; a `start→end` label moves
    /// as one song, by one number), or in another mode (no transposition).
    /// Not a key: as charted.
    #[must_use]
    pub fn played(&self, key: &str) -> Played {
        let (Some((to, _)), Some((from, _))) = (pitch(key), pitch(&self.start)) else {
            return Played::AsCharted;
        };
        if minor(key) != minor(&self.start) {
            return Played::OtherMode(Self::plain(key.trim()));
        }
        let step = semitones(from, to);
        if step == 0 {
            return Played::AsCharted;
        }
        self.in_key(key)
            .map_or(Played::AsCharted, |label| Played::Moved {
                label,
                semitones: step,
            })
    }

    /// How big its type can be on a 48 px tile: `""`, `s`, or `xs`.
    #[must_use]
    pub fn size(&self) -> &'static str {
        match self.text().chars().count() {
            0..=3 => "",
            4..=5 => "s",
            _ => "xs",
        }
    }
}

/// The key facts of a parsed chart: where it starts and ends, and whether
/// it is plainly in its key. None for a chart with no key.
#[cfg(feature = "engrave")]
#[must_use]
pub fn facts(chart: &keyflow::Chart) -> Option<KeyLabel> {
    let start_key = chart.initial_key.as_ref()?;
    let end_key = chart
        .ending_key
        .as_ref()
        .or_else(|| chart.key_changes.last().map(|k| &k.to_key))
        .unwrap_or(start_key);
    let start = start_key.short_name();
    let end = end_key.short_name();
    let returns = !chart.key_changes.is_empty() && start == end;
    let outside = outside_chords(chart) >= OUTSIDE;
    Some(KeyLabel {
        end: (end != start).then_some(end),
        outside: start == end_key.short_name() && (returns || outside),
        start,
    })
}

/// How many different chords of `chart` lie outside the key in force where
/// they are played (by root and triad), a secondary dominant not counted.
#[cfg(feature = "engrave")]
#[must_use]
pub fn outside_chords(chart: &keyflow::Chart) -> usize {
    use keyflow::chord::ChordQuality;
    let at = |p: &keyflow::time::AbsolutePosition| {
        let t = &p.total_duration;
        (t.measure, t.beat, t.subdivision)
    };
    let triad = |q: &ChordQuality| match q {
        ChordQuality::Major => Some(0),
        ChordQuality::Minor => Some(1),
        ChordQuality::Diminished => Some(2),
        ChordQuality::Augmented => Some(3),
        _ => None,
    };
    let mut chords: Vec<&keyflow::chart::ChordInstance> = chart
        .sections
        .iter()
        .flat_map(|s| s.tracks.iter())
        .flat_map(|t| t.measures.iter())
        .flat_map(|m| m.chords.iter())
        .collect();
    chords.sort_by_key(|c| at(&c.position));
    let key_at = |c: &keyflow::chart::ChordInstance| {
        chart
            .key_changes
            .iter()
            .rfind(|k| at(&k.position) <= at(&c.position))
            .map(|k| &k.to_key)
            .or(chart.initial_key.as_ref())
    };
    let root_of = |c: &keyflow::chart::ChordInstance| {
        key_at(c)
            .and_then(|k| c.root.resolve(Some(k)))
            .map(|n| i32::from(n.semitone))
    };
    let mut found: Vec<(i32, Option<u8>)> = Vec::new();
    for (i, chord) in chords.iter().enumerate() {
        let (Some(key), Some(root)) = (key_at(chord), root_of(chord)) else {
            continue;
        };
        let scale: Vec<i32> = key.notes().iter().map(|n| i32::from(n.semitone)).collect();
        let quality = triad(&chord.parsed.quality);
        let degree = scale.iter().position(|pc| *pc == root);
        let off = degree.is_none_or(|d| {
            let want = key
                .diatonic_quality(u8::try_from(d.saturating_add(1)).unwrap_or(0))
                .as_ref()
                .and_then(triad);
            quality.is_some() && want.is_some() && quality != want
        });
        if !off {
            continue;
        }
        // A secondary dominant: major, resolving up a fourth to the key.
        let next_root = chords.get(i.saturating_add(1)).and_then(|n| root_of(n));
        let secondary = quality == Some(0)
            && next_root
                .is_some_and(|n| n.wrapping_sub(root).rem_euclid(12) == 5 && scale.contains(&n));
        if secondary {
            continue;
        }
        let it = (root, quality.map(|q: u8| q));
        if !found.contains(&it) {
            found.push(it);
        }
    }
    found.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_says_its_key_and_what_is_less_plain() {
        assert_eq!(KeyLabel::plain("E").text(), "E");
        assert_eq!(
            KeyLabel {
                start: "E".into(),
                end: None,
                outside: true
            }
            .text(),
            "E*"
        );
        let moving = KeyLabel {
            start: "E".into(),
            end: Some("F".into()),
            outside: true,
        };
        assert_eq!(moving.text(), "E→F", "an end takes the asterisk's place");
        assert_eq!(
            KeyLabel::parse("E→F"),
            Some(KeyLabel {
                start: "E".into(),
                end: Some("F".into()),
                outside: false
            })
        );
        assert_eq!(
            KeyLabel::parse("F#m*").map(|k| (k.start, k.outside)),
            Some(("F#m".to_owned(), true))
        );
        assert_eq!(KeyLabel::parse(" "), None);
        assert_eq!(
            KeyLabel::parse("E*").unwrap().spoken(),
            "Key of E, with notes outside the key"
        );
        assert_eq!(
            KeyLabel::parse("E→F").unwrap().spoken(),
            "Starts in E, ends in F"
        );
        assert_eq!(
            KeyLabel::parse("F#m").unwrap().spoken(),
            "Key of F sharp minor"
        );
        assert_eq!(KeyLabel::parse("Eb→F#m").unwrap().size(), "xs");
        assert_eq!(KeyLabel::parse("E*").unwrap().size(), "");
    }

    const NAMES: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
    ];

    #[test]
    fn every_interval_both_ways_the_shorter_way() {
        for (from, a) in NAMES.iter().enumerate() {
            for up in 0..12_i32 {
                let to = NAMES[(from + usize::try_from(up).unwrap()) % 12];
                let want = if up > 6 { up - 12 } else { up };
                let got = KeyLabel::plain(a).played(to);
                if want == 0 {
                    assert_eq!(got, Played::AsCharted, "{a}→{to}");
                } else {
                    let Played::Moved { semitones, label } = got else {
                        panic!("{a}→{to}: {got:?}")
                    };
                    assert_eq!(semitones, want, "{a}→{to}");
                    assert_eq!(label.start, to);
                    // And back the other way: the same distance, negated
                    // (a tritone is +6 both ways).
                    let Played::Moved {
                        semitones: back, ..
                    } = KeyLabel::plain(to).played(a)
                    else {
                        panic!("{to}→{a}")
                    };
                    assert_eq!(back, if want == 6 { 6 } else { -want }, "{to}→{a}");
                }
            }
        }
        assert_eq!(semitones(2, 6), 4, "D→F# up 4");
        assert_eq!(semitones(7, 4), -3, "G→E down 3");
        assert_eq!(semitones(0, 6), 6, "the tritone, up");
        assert_eq!(semitones(6, 0), 6, "the tritone, up again");
        assert_eq!(semitones(0, 7), -5, "a fifth up is a fourth down");
    }

    #[test]
    fn enharmonic_keys_are_no_transposition() {
        for (a, b) in [
            ("F#", "Gb"),
            ("Gb", "F#"),
            ("C#", "Db"),
            ("A#m", "Bbm"),
            ("D#m", "Ebm"),
            ("Cb", "B"),
            ("E#", "F"),
        ] {
            assert_eq!(KeyLabel::plain(a).played(b), Played::AsCharted, "{a} = {b}");
        }
    }

    #[test]
    fn minor_keys_move_by_their_tonic_and_a_mode_change_is_no_transposition() {
        let Played::Moved { label, semitones } = KeyLabel::plain("Am").played("Cm") else {
            panic!()
        };
        assert_eq!((label.text().as_str(), semitones), ("Cm", 3));
        let Played::Moved { label, semitones } = KeyLabel::plain("F#m").played("Em") else {
            panic!()
        };
        assert_eq!((label.text().as_str(), semitones), ("Em", -2));
        assert_eq!(
            KeyLabel::plain("E").played("Em"),
            Played::OtherMode(KeyLabel::plain("Em"))
        );
        assert_eq!(
            KeyLabel::plain("Em").played("G"),
            Played::OtherMode(KeyLabel::plain("G"))
        );
        assert_eq!(
            KeyLabel::plain("E").played("Emaj"),
            Played::AsCharted,
            "maj is major"
        );
        assert_eq!(
            KeyLabel::plain("E").played("H"),
            Played::AsCharted,
            "no key"
        );
    }

    #[test]
    fn a_start_end_label_moves_as_one_song_by_one_number() {
        let label = KeyLabel {
            start: "D".into(),
            end: Some("E".into()),
            outside: false,
        };
        let Played::Moved {
            label: moved,
            semitones,
        } = label.played("Eb")
        else {
            panic!()
        };
        assert_eq!((moved.text().as_str(), semitones), ("Eb→F", 1));
        let Played::Moved {
            label: moved,
            semitones,
        } = label.played("Bb")
        else {
            panic!()
        };
        assert_eq!((moved.text().as_str(), semitones), ("Bb→C", -4));
        let star = KeyLabel {
            start: "G".into(),
            end: None,
            outside: true,
        };
        let Played::Moved {
            label: moved,
            semitones,
        } = star.played("A")
        else {
            panic!()
        };
        assert_eq!((moved.text().as_str(), semitones), ("A*", 2));
    }

    #[test]
    fn played_in_another_key_every_key_moves() {
        let label = KeyLabel {
            start: "B".into(),
            end: Some("C".into()),
            outside: false,
        };
        let moved = label.in_key("Bb").unwrap();
        assert_eq!(moved.text(), "Bb→B");
        assert_eq!(KeyLabel::plain("D").in_key("E").unwrap().text(), "E");
        assert_eq!(
            KeyLabel {
                start: "Am".into(),
                end: Some("Bm".into()),
                outside: false
            }
            .in_key("Gm")
            .unwrap()
            .text(),
            "Gm→Am"
        );
        assert_eq!(
            KeyLabel::plain("E").in_key("E"),
            None,
            "the same key is no transposition"
        );
        assert_eq!(KeyLabel::plain("E").in_key("H"), None);
    }

    /// The five Merry & Bright charts (read from the owner's vault copy when
    /// it is on this machine): what each row says.
    #[cfg(feature = "engrave")]
    #[test]
    fn the_merry_and_bright_charts() {
        let dir = std::path::Path::new(
            "/run/media/Development/setlists/merry-and-bright/vault/Libraries/Christmas",
        );
        if !dir.is_dir() {
            eprintln!("skipped: no vault copy here");
            return;
        }
        let mut seen = Vec::new();
        for song in [
            "Sleigh Ride",
            "Silent Night",
            "Joy to the World (Unspeakable Joy)",
            "This Christmas",
            "All I Want for Christmas Is You",
        ] {
            let Ok(source) = std::fs::read_to_string(dir.join(song).join(format!("{song}.kf")))
            else {
                continue;
            };
            let chart = keyflow::parse(source.as_str()).expect("it parses");
            let label = facts(&chart).expect("a key");
            eprintln!(
                "{song}: {} (changes {}, outside chords {})",
                label.text(),
                chart.key_changes.len(),
                outside_chords(&chart)
            );
            seen.push((song, label.text()));
        }
        if seen.len() == 5 {
            assert_eq!(
                seen,
                [
                    ("Sleigh Ride", "B→C".to_owned()),
                    ("Silent Night", "A".to_owned()),
                    ("Joy to the World (Unspeakable Joy)", "D→E".to_owned()),
                    ("This Christmas", "F*".to_owned()),
                    ("All I Want for Christmas Is You", "G*".to_owned()),
                ]
            );
        }
    }
}
