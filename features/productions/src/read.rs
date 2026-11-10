//! What each adapter reads out of a production's file. Read only: no
//! adapter's writer is called from here.
//!
//! | | tempo | key | length | tracks | plugins | missing media |
//! |---|---|---|---|---|---|---|
//! | Ableton (`ableton`) | yes | Live 11+ sets that have one | yes | yes | yes | yes |
//! | Pro Tools (`protools`) | yes | no | yes | yes | yes | yes |
//! | REAPER (`dawfile-reaper`) | yes | no | yes | yes | yes | yes |
//! | Logic (`logic`) | yes | yes | no | yes | no | no |
//!
//! - **Length** is the end of the last thing on the timeline: Live's
//!   furthest bar through its tempo map, Pro Tools' last audio region,
//!   REAPER's last item. Logic's regions are not decoded.
//! - **Tracks** are the ones a person made: Live's audio, MIDI and group
//!   tracks (returns and the master left out), Pro Tools' audio and MIDI
//!   tracks, every REAPER track, Logic's mixer tracks.
//! - **Plugins** are third-party ones by name (Live's own devices left
//!   out); Pro Tools keeps one list for the whole session.
//! - **Missing media** is each audio file the project names that is not
//!   where it says: Live's sample paths (absolute, then relative to the
//!   project), Pro Tools' file names under `Audio Files/` (it stores no
//!   path), REAPER's source paths.
//! - **Last saved** is the file's modification time ([`crate::find`]);
//!   no adapter reads a save time out of the file.

use std::collections::BTreeSet;
use std::path::Path;

use crate::find::Found;
use crate::model::Daw;

/// A song of an Ableton show set.
#[derive(Debug, Clone, PartialEq)]
pub struct ShowSong {
    pub name: String,
    /// The tempo where it starts.
    pub tempo: f64,
    /// How long it plays, in seconds.
    pub seconds: f64,
}

/// What was read. `None` is "this adapter cannot say", never "none".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub tempo: Option<f64>,
    pub key: Option<String>,
    /// In seconds.
    pub length: Option<f64>,
    pub tracks: Option<usize>,
    pub plugins: Option<Vec<String>>,
    pub missing_media: Option<Vec<String>>,
    /// An Ableton set's locators read as songs (`ableton::setlist::songs`).
    pub songs: Vec<ShowSong>,
    /// The set marks itself as a show: a locator ends a song (`SONG
    /// END`, `END`, `STOP`) or names a section (`>Verse`).
    pub marked_show: bool,
    /// Why the file did not read; the rest is then empty.
    pub error: Option<String>,
}

impl Facts {
    /// Whether its songs are a setlist: two or more, in a set that marks
    /// itself as a show or that a person said is one.
    #[must_use]
    pub const fn is_show(&self, said: bool) -> bool {
        self.songs.len() >= 2 && (self.marked_show || said)
    }
}

/// The sample rate Pro Tools' positions are read at.
const PT_RATE: u32 = 48_000;

fn sorted(names: BTreeSet<String>) -> Vec<String> {
    names.into_iter().collect()
}

fn positive(value: f64) -> Option<f64> {
    (value.is_finite() && value > 0.0).then_some(value)
}

/// `C`, `F#m`, or `D Dorian`: a tonic and a scale as a chart writes a key.
fn key_name(tonic: &str, scale: &str) -> String {
    let scale = scale.trim();
    if scale.is_empty() || scale.eq_ignore_ascii_case("major") {
        tonic.to_owned()
    } else if scale.eq_ignore_ascii_case("minor") {
        format!("{tonic}m")
    } else {
        format!("{tonic} {scale}")
    }
}

fn ableton(found: &Found) -> Result<Facts, String> {
    use ableton::setlist::songs;
    use ableton::tempo::TempoMap;
    use ableton::{Device, DeviceFormat};

    let set = ableton::read_live_set(&found.file).map_err(|e| e.to_string())?;
    let map = TempoMap::of(&set);

    let made = set
        .audio_tracks
        .len()
        .saturating_add(set.midi_tracks.len())
        .saturating_add(set.group_tracks.len());

    let chains = set
        .audio_tracks
        .iter()
        .map(|t| &t.common.devices)
        .chain(set.midi_tracks.iter().map(|t| &t.common.devices))
        .chain(set.group_tracks.iter().map(|t| &t.common.devices))
        .chain(set.return_tracks.iter().map(|t| &t.common.devices))
        .chain(set.master_track.iter().map(|t| &t.devices));
    let plugins: BTreeSet<String> = chains
        .flatten()
        .filter(|d: &&Device| !matches!(d.format, DeviceFormat::Builtin))
        .map(|d| d.name.trim().to_owned())
        .filter(|n| !n.is_empty())
        .collect();

    let base = found.file.parent().unwrap_or_else(|| Path::new("."));
    let mut missing = BTreeSet::new();
    let clips = set.audio_tracks.iter().flat_map(|t| {
        t.arrangement_clips
            .iter()
            .chain(t.session_clips.iter().map(|s| &s.clip))
    });
    for sample in clips.filter_map(|c| c.sample_ref.as_ref()) {
        let relative = sample.relative_path.as_ref().map(|p| base.join(p));
        let places: Vec<&Path> = [sample.path.as_deref(), relative.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        if places.is_empty() || places.iter().any(|p| p.exists()) {
            continue;
        }
        let name = sample
            .name
            .clone()
            .filter(|n| !n.is_empty())
            .or_else(|| {
                places
                    .first()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        if !name.is_empty() {
            missing.insert(name);
        }
    }

    let end_beats = set.furthest_bar * f64::from(set.time_signature.numerator.max(1));
    let marked_show = set.locators.iter().any(|l| {
        let name = l.name.trim();
        name.starts_with('>')
            || matches!(
                name.to_ascii_uppercase().as_str(),
                "SONG END" | "END" | "STOP" | "SONG STOP"
            )
    });
    Ok(Facts {
        tempo: positive(set.tempo),
        key: set
            .key_signature
            .as_ref()
            .map(|k| key_name(&k.root_note.to_string(), &k.scale)),
        length: positive(map.seconds_at(end_beats)),
        tracks: Some(made),
        plugins: Some(sorted(plugins)),
        missing_media: Some(sorted(missing)),
        songs: songs(&set)
            .into_iter()
            .map(|s| ShowSong {
                tempo: map.bpm_at(s.start_beats),
                seconds: s.seconds(),
                name: s.name,
            })
            .collect(),
        marked_show,
        error: None,
    })
}

/// `samples` at [`PT_RATE`] as seconds.
fn pt_seconds(samples: u64) -> Option<f64> {
    let rate = u64::from(PT_RATE);
    let whole = u32::try_from(samples.checked_div(rate)?).ok()?;
    let rest = u32::try_from(samples.checked_rem(rate)?).ok()?;
    Some(f64::from(whole) + f64::from(rest) / f64::from(PT_RATE))
}

fn pro_tools(found: &Found) -> Result<Facts, String> {
    let session = protools::read_session(&found.file, PT_RATE).map_err(|e| e.to_string())?;
    let dir = if found.location.is_dir() {
        found.location.as_path()
    } else {
        found.file.parent().unwrap_or_else(|| Path::new("."))
    };
    let missing: BTreeSet<String> = session
        .audio_files
        .iter()
        .map(|f| f.filename.trim())
        .filter(|n| !n.is_empty())
        .filter(|n| !dir.join("Audio Files").join(n).exists() && !dir.join(n).exists())
        .map(str::to_owned)
        .collect();
    let end = session
        .audio_regions
        .iter()
        .map(|r| r.start_pos.saturating_add(r.length))
        .max();
    Ok(Facts {
        tempo: positive(session.bpm),
        key: None,
        length: end.and_then(pt_seconds).and_then(positive),
        tracks: Some(
            session
                .audio_tracks
                .len()
                .saturating_add(session.midi_tracks.len()),
        ),
        plugins: Some(sorted(
            session
                .plugins
                .iter()
                .map(|p| p.name.trim().to_owned())
                .filter(|n| !n.is_empty())
                .collect(),
        )),
        missing_media: Some(sorted(missing)),
        ..Facts::default()
    })
}

fn reaper(found: &Found) -> Result<Facts, String> {
    use dawfile_reaper::ProjectIndex;
    use dawfile_reaper::types::item::SourceType;

    let project = dawfile_reaper::read_project(&found.file).map_err(|e| e.to_string())?;
    let index = ProjectIndex::build(&project);
    let base = found.file.parent().unwrap_or_else(|| Path::new("."));
    let items = || {
        project
            .tracks
            .iter()
            .flat_map(|t| &t.items)
            .chain(&project.items)
    };
    let end = items()
        .map(|i| i.position + i.length)
        .fold(0.0_f64, f64::max);
    let missing: BTreeSet<String> = items()
        .flat_map(|i| &i.takes)
        .filter_map(|t| t.source.as_ref())
        .filter(|s| {
            !matches!(
                s.source_type,
                SourceType::Midi | SourceType::Empty | SourceType::Section
            )
        })
        .map(|s| s.file_path.trim())
        .filter(|p| !p.is_empty())
        .filter(|p| !Path::new(p).exists() && !base.join(p).exists())
        .map(|p| {
            Path::new(p)
                .file_name()
                .map_or_else(|| p.to_owned(), |n| n.to_string_lossy().into_owned())
        })
        .collect();
    Ok(Facts {
        tempo: project.properties.tempo.and_then(|t| positive(t.0)),
        key: None,
        length: positive(end),
        tracks: usize::try_from(index.summary().tracks).ok(),
        plugins: Some(index.plugin_inventory().to_vec()),
        missing_media: Some(sorted(missing)),
        ..Facts::default()
    })
}

fn logic_pro(found: &Found) -> Result<Facts, String> {
    let session = logic::read_session(&found.file).map_err(|e| e.to_string())?;
    let tonic = session.key.trim();
    Ok(Facts {
        tempo: positive(session.bpm),
        key: (!tonic.is_empty()).then(|| key_name(tonic, &session.key_gender)),
        tracks: Some(session.tracks.len()),
        ..Facts::default()
    })
}

/// What `found`'s adapter reads out of its file. A file that does not
/// read is still a production: its facts are empty but for the reason.
#[must_use]
pub fn read(found: &Found) -> Facts {
    let facts = match found.daw {
        Daw::Ableton => ableton(found),
        Daw::ProTools => pro_tools(found),
        Daw::Reaper => reaper(found),
        Daw::Logic => logic_pro(found),
    };
    facts.unwrap_or_else(|error| Facts {
        error: Some(error),
        ..Facts::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_as_a_chart_writes_them() {
        assert_eq!(key_name("C", "Major"), "C");
        assert_eq!(key_name("F#", "minor"), "F#m");
        assert_eq!(key_name("D", "Dorian"), "D Dorian");
        assert_eq!(pt_seconds(72_000), Some(1.5));
    }

    #[test]
    fn a_show_is_two_songs_marked_or_said() {
        let song = |name: &str| ShowSong {
            name: name.to_owned(),
            tempo: 120.0,
            seconds: 60.0,
        };
        let mut facts = Facts {
            songs: vec![song("One"), song("Two")],
            ..Facts::default()
        };
        assert!(!facts.is_show(false));
        assert!(facts.is_show(true));
        facts.marked_show = true;
        assert!(facts.is_show(false));
        facts.songs.pop();
        assert!(!facts.is_show(true));
    }
}
