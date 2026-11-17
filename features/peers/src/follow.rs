//! Following another engine: from what it plays ([`NowPlaying`]) to what
//! this engine does, in pure steps an app carries out.
//!
//! The rules, the same whatever either engine runs:
//!
//! - **The song** is matched by title in this engine's set (case and
//!   punctuation aside); a song this set lacks is said, never guessed.
//! - **The section**, when the source names one, picks this engine's part
//!   of that name, if the song has one.
//! - **The cue** (a rig patch or scene the set names at a point in the
//!   song): the source's own, or the one this engine's song lists at the
//!   source's position ([`Here::cues`]); each one once, as it is crossed.
//! - **The tempo** follows when it moves by more than a hundredth of a beat.
//! - **The transport**: play and stop follow, and the position is put
//!   right when it drifts past [`DRIFT`] (or on a song change), not every
//!   tick — a rig needs the beat for its delays, not a sample-locked clock.

use crate::wire::NowPlaying;

/// How far this engine's playhead may drift from the source's before it
/// is moved, in seconds.
pub const DRIFT: f64 = 0.25;

/// This engine as the follow sees it: its set's songs, the song up, its
/// parts, its transport.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Here {
    /// The set that is up, in order.
    pub songs: Vec<String>,
    pub song_index: usize,
    /// The song's parts (sections).
    pub parts: Vec<String>,
    pub part_index: usize,
    pub playing: bool,
    pub position: f64,
    pub bpm: f64,
    /// The song's rig cues: seconds into the song, and the patch or scene
    /// each names, in order.
    pub cues: Vec<(f64, String)>,
}

/// The cue in force at `t` seconds into the song: the last one at or
/// before it.
#[must_use]
pub fn cue_at(cues: &[(f64, String)], t: f64) -> Option<&str> {
    cues.iter()
        .take_while(|(at, _)| *at <= t + 1e-6)
        .last()
        .map(|(_, name)| name.as_str())
}

/// One thing to do so this engine follows.
#[derive(Clone, Debug, PartialEq)]
pub enum FollowAction {
    /// Put up song `index` of this engine's set (`title` as the source
    /// named it).
    Song { index: usize, title: String },
    /// The source's song is not in this engine's set.
    Missing { title: String },
    /// Play part `index` of the song up.
    Part { index: usize, name: String },
    /// Play the patch or scene this cue names.
    Cue { name: String },
    Tempo(f64),
    Play,
    Stop,
    Seek(f64),
}

/// The follow's memory: what it last saw of the source, so each step does
/// only what changed.
#[derive(Clone, Debug, Default)]
pub struct Follow {
    last: Option<NowPlaying>,
    cue: Option<String>,
}

/// A title as compared: letters and digits, lower case. "Hey!" is "hey",
/// "WORST PART OF YOU" is "worstpartofyou".
#[must_use]
pub fn title_key(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Where `title` is in `songs`: the same title, else the only song whose
/// title starts with it or that it starts with ("Know Me" and "Know Me
/// (Extended)").
#[must_use]
pub fn find_song(songs: &[String], title: &str) -> Option<usize> {
    let want = title_key(title);
    if want.is_empty() {
        return None;
    }
    if let Some(i) = songs.iter().position(|s| title_key(s) == want) {
        return Some(i);
    }
    let close: Vec<usize> = songs
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let k = title_key(s);
            !k.is_empty() && (k.starts_with(&want) || want.starts_with(&k))
        })
        .map(|(i, _)| i)
        .collect();
    if let [one] = close.as_slice() {
        return Some(*one);
    }
    // One title inside the other ("Cinematic Intro", "DTLA Cinematic Intro").
    let inside: Vec<usize> = songs
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let k = title_key(s);
            !k.is_empty() && (k.contains(&want) || want.contains(&k))
        })
        .map(|(i, _)| i)
        .collect();
    match inside.as_slice() {
        [one] => Some(*one),
        _ => None,
    }
}

impl Follow {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget the source: the next step acts as if it were the first.
    pub fn reset(&mut self) {
        self.last = None;
        self.cue = None;
    }

    /// What to do so `here` follows `source`, at `now_ms` on the source's
    /// clock (the follower's own clock is close enough on one LAN).
    pub fn step(&mut self, source: &NowPlaying, here: &Here, now_ms: u64) -> Vec<FollowAction> {
        let mut out = Vec::new();
        let first = self.last.is_none();
        let last = self.last.clone().unwrap_or_default();

        // The song.
        let mut song_moved = false;
        if !source.song.is_empty() && (first || title_key(&source.song) != title_key(&last.song)) {
            match find_song(&here.songs, &source.song) {
                Some(index) => {
                    if index != here.song_index || first {
                        song_moved = index != here.song_index;
                        out.push(FollowAction::Song {
                            index,
                            title: source.song.clone(),
                        });
                    }
                }
                None => out.push(FollowAction::Missing {
                    title: source.song.clone(),
                }),
            }
        }

        // The section: only once the song is this engine's too.
        if !source.section.is_empty()
            && (first || song_moved || title_key(&source.section) != title_key(&last.section))
        {
            let want = title_key(&source.section);
            if let Some(index) = here.parts.iter().position(|p| title_key(p) == want) {
                if index != here.part_index || song_moved {
                    out.push(FollowAction::Part {
                        index,
                        name: source.section.clone(),
                    });
                }
            }
        }

        // The cue: on a song change, or as the playhead crosses one.
        let at = source.position_at(now_ms);
        let cue = if source.cue.is_empty() {
            cue_at(&here.cues, at).map(str::to_owned)
        } else {
            Some(source.cue.clone())
        };
        if song_moved {
            self.cue = None;
        }
        if let Some(cue) = cue.filter(|c| !c.is_empty()) {
            if self.cue.as_deref() != Some(cue.as_str()) {
                out.push(FollowAction::Cue { name: cue.clone() });
                self.cue = Some(cue);
            }
        }

        // The tempo.
        if source.bpm > 0.0 && (source.bpm - here.bpm).abs() > 0.01 {
            out.push(FollowAction::Tempo(source.bpm));
        }

        // The transport.
        if source.playing && !here.playing {
            out.push(FollowAction::Seek(at));
            out.push(FollowAction::Play);
        } else if !source.playing && here.playing {
            out.push(FollowAction::Stop);
            out.push(FollowAction::Seek(source.position));
        } else if song_moved || (at - here.position).abs() > DRIFT {
            out.push(FollowAction::Seek(at));
        }

        self.last = Some(source.clone());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recital, in order.
    fn recital() -> Vec<String> {
        [
            "DTLA Cinematic Intro",
            "Drive To LA",
            "For You",
            "Heart Isn't Ready",
            "Hey!",
            "She's American",
            "WORST PART OF YOU",
            "Fire",
            "Not Too Much Longer (Live Arrangement)",
            "Know Me (Extended)",
        ]
        .map(String::from)
        .to_vec()
    }

    fn here() -> Here {
        Here {
            songs: recital(),
            parts: vec!["Intro".into(), "Verse".into(), "Chorus".into()],
            bpm: 120.0,
            ..Here::default()
        }
    }

    fn playing(song: &str, section: &str, position: f64, at_ms: u64) -> NowPlaying {
        NowPlaying {
            song: song.into(),
            section: section.into(),
            playing: true,
            position,
            bpm: 120.0,
            at_ms,
            ..NowPlaying::default()
        }
    }

    #[test]
    fn titles_match_whatever_the_case_and_punctuation() {
        let set = recital();
        assert_eq!(find_song(&set, "hey"), Some(4));
        assert_eq!(find_song(&set, "Worst Part Of You"), Some(6));
        assert_eq!(find_song(&set, "Heart Isnt Ready"), Some(3));
        // A source that calls it by its short title.
        assert_eq!(find_song(&set, "Know Me"), Some(9));
        assert_eq!(find_song(&set, "Not Too Much Longer"), Some(8));
        // The show's locators name some songs shorter.
        assert_eq!(find_song(&set, "Cinematic Intro"), Some(0));
        assert_eq!(find_song(&set, "Heart isn't Ready"), Some(3));
        assert_eq!(find_song(&set, "Yellow"), None);
        assert_eq!(find_song(&set, ""), None);
    }

    #[test]
    fn a_song_change_puts_the_song_up_and_the_playhead_where_the_source_is() {
        let mut f = Follow::new();
        let mut h = here();
        let src = playing("For You", "Verse", 10.0, 1_000);
        let acts = f.step(&src, &h, 1_500);
        assert_eq!(
            acts,
            vec![
                FollowAction::Song {
                    index: 2,
                    title: "For You".into()
                },
                FollowAction::Part {
                    index: 1,
                    name: "Verse".into()
                },
                FollowAction::Seek(10.5),
                FollowAction::Play,
            ]
        );
        // Carried out.
        h.song_index = 2;
        h.part_index = 1;
        h.playing = true;
        h.position = 10.5;
        // Nothing moved: nothing to do.
        assert!(f.step(&src, &h, 1_500).is_empty());
        // The chorus.
        let chorus = NowPlaying {
            section: "Chorus".into(),
            position: 30.0,
            at_ms: 21_000,
            ..src.clone()
        };
        h.position = 30.0;
        assert_eq!(
            f.step(&chorus, &h, 21_000),
            vec![FollowAction::Part {
                index: 2,
                name: "Chorus".into()
            }]
        );
    }

    #[test]
    fn the_rig_plays_each_cue_as_the_playhead_crosses_it() {
        let mut f = Follow::new();
        let mut h = here();
        h.songs.push("Sad But True".into());
        h.song_index = 10;
        h.playing = true;
        h.cues = vec![
            (0.0, "Sad But True - Main".into()),
            (60.0, "Verses".into()),
            (90.0, "Sad But True - Main".into()),
        ];
        let at = |position: f64| NowPlaying {
            song: "Sad But True".into(),
            playing: true,
            position,
            bpm: 120.0,
            ..NowPlaying::default()
        };
        let cue = |name: &str| FollowAction::Cue { name: name.into() };
        h.position = 1.0;
        assert_eq!(f.step(&at(1.0), &h, 0), vec![
            FollowAction::Song { index: 10, title: "Sad But True".into() },
            cue("Sad But True - Main"),
        ]);
        h.position = 30.0;
        assert!(f.step(&at(30.0), &h, 0).is_empty(), "inside one cue, nothing");
        h.position = 61.0;
        assert_eq!(f.step(&at(61.0), &h, 0), vec![cue("Verses")]);
        h.position = 95.0;
        assert_eq!(f.step(&at(95.0), &h, 0), vec![cue("Sad But True - Main")]);
        // A source that names its cue wins.
        let named = NowPlaying { cue: "Solo".into(), ..at(95.0) };
        assert_eq!(f.step(&named, &h, 0), vec![cue("Solo")]);
        assert_eq!(cue_at(&h.cues, 59.9), Some("Sad But True - Main"));
        assert_eq!(cue_at(&[], 1.0), None);
    }

    #[test]
    fn stop_tempo_and_drift_follow_and_a_missing_song_is_said() {
        let mut f = Follow::new();
        let mut h = here();
        h.song_index = 4;
        h.playing = true;
        let src = NowPlaying {
            song: "Hey!".into(),
            bpm: 92.0,
            playing: false,
            position: 42.0,
            ..NowPlaying::default()
        };
        assert_eq!(
            f.step(&src, &h, 0),
            vec![
                FollowAction::Song {
                    index: 4,
                    title: "Hey!".into()
                },
                FollowAction::Tempo(92.0),
                FollowAction::Stop,
                FollowAction::Seek(42.0),
            ]
        );
        h.playing = false;
        h.bpm = 92.0;
        h.position = 42.0;
        // Drift past the bound while both play is put right; inside it, not.
        let rolling = NowPlaying {
            playing: true,
            at_ms: 0,
            ..src.clone()
        };
        h.playing = true;
        h.position = 42.1;
        assert!(f.step(&rolling, &h, 0).is_empty());
        h.position = 43.0;
        assert_eq!(f.step(&rolling, &h, 0), vec![FollowAction::Seek(42.0)]);
        let missing = NowPlaying {
            song: "Yellow".into(),
            ..rolling
        };
        h.position = 42.0;
        assert_eq!(
            f.step(&missing, &h, 0),
            vec![FollowAction::Missing {
                title: "Yellow".into()
            }]
        );
    }
}
