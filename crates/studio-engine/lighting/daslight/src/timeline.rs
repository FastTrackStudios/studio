//! Super Scenes: a timeline of audio and scenes, played as one scene.
//!
//! A Super Scene is a `SCENE` with no fixture values whose only rack is a
//! timeline (`RACK TYPE="1"`), as the desk saves one:
//!
//! ```text
//! RACK TYPE="1" DURATION=<ticks> DISPLAY_MODE="1" GRID_BPM GRID_OFFSET="0"
//!   TIMELINES
//!     TIMELINE DASUID NAME=<index> INDEX DASTLLOCKED DASTLMUTED DASTLFOLDED
//!       BLOCKS
//!         BLOCK TYPE="2"  an audio file:  START END (ms) POSITION FADEIN
//!                         FADEOUT DASTLMEDIAPATH DASTLHEIGHT BPM ALLOWLOOP
//!                         CONFORM_TO_TEMPO
//!         BLOCK TYPE="1"  a scene:        START END (ms) POSITION FADEIN
//!                         FADEOUT SPEED ALLOWLOOP CONFORM_TO_TEMPO SCENEUUID
//!           DASTLSUBLINES DASTLNB="0"
//! ```
//!
//! `DURATION` is in the desk's 1/25 s ticks; a block's `START` and `END`
//! are milliseconds on the timeline. A saved Super Scene ends with one
//! empty timeline below the last used one, which is kept.
//!
//! What plays when is the song's: [`WarpMap`] turns a song beat into a
//! second of its audio file, from the warp markers the DAW holds for it.

use crate::cook::TICKS_PER_SECOND;
use crate::patch::stable_uid;
use crate::scene::Scene;
use crate::xml::Element;

/// One warp marker: this second of the audio file plays this beat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WarpMarker {
    pub seconds: f64,
    pub beat: f64,
}

/// Where each beat falls in an audio file: straight lines between warp
/// markers (as a DAW plays a warped clip), the first and last segments
/// carried on past the ends.
#[derive(Debug, Clone, PartialEq)]
pub struct WarpMap {
    markers: Vec<WarpMarker>,
}

impl WarpMap {
    /// The map through `markers`, put in beat order. `None` with fewer
    /// than two markers or two on the same beat: no tempo to go on.
    #[must_use]
    pub fn new(mut markers: Vec<WarpMarker>) -> Option<Self> {
        markers.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        let distinct = markers.windows(2).all(|w| match w {
            [a, b] => b.beat > a.beat,
            _ => true,
        });
        (markers.len() >= 2 && distinct).then_some(Self { markers })
    }

    /// The second of the audio at which `beat` (on the markers' count)
    /// plays; a beat before the audio starts is its first second, 0.
    #[must_use]
    pub fn seconds_at(&self, beat: f64) -> f64 {
        // The segment the beat is in, or the nearest end one.
        let segment = self
            .markers
            .windows(2)
            .find(|w| matches!(w, [_, b] if beat <= b.beat))
            .or_else(|| self.markers.windows(2).last());
        let Some([a, b]) = segment else {
            return 0.0;
        };
        let t = (beat - a.beat) / (b.beat - a.beat);
        t.mul_add(b.seconds - a.seconds, a.seconds).max(0.0)
    }

    /// The mean tempo over the markers, beats a minute.
    #[must_use]
    pub fn mean_bpm(&self) -> f64 {
        match (self.markers.first(), self.markers.last()) {
            (Some(a), Some(b)) if b.seconds > a.seconds => {
                (b.beat - a.beat) * 60.0 / (b.seconds - a.seconds)
            }
            _ => 120.0,
        }
    }
}

/// What a block plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// An audio file, by its path on the desk's computer.
    Audio { path: String },
    /// A scene, by its `DASUID` ([`Scene::uid`]).
    Scene { uid: String },
}

/// One block on a timeline, in milliseconds from the timeline's start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub content: Content,
    pub start: u32,
    pub end: u32,
    pub fade_in: u32,
    pub fade_out: u32,
}

/// A Super Scene: its timelines top to bottom (0 is the first), and the
/// grid's tempo.
#[derive(Debug, Clone, PartialEq)]
pub struct SuperScene {
    pub name: String,
    pub color: String,
    pub bpm: f64,
    pub tracks: Vec<Vec<Block>>,
}

/// Milliseconds in whole `seconds`, rounded; negative is 0.
#[must_use]
pub fn millis(seconds: f64) -> u32 {
    format!("{:.0}", (seconds * 1000.0).max(0.0))
        .parse()
        .unwrap_or(u32::MAX)
}

/// A tempo as the desk writes one: three decimals at most.
fn bpm_text(bpm: f64) -> String {
    let text = format!("{bpm:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.to_owned()
}

impl SuperScene {
    /// The timeline's length: the end of its last block, in ms.
    #[must_use]
    pub fn length(&self) -> u32 {
        self.tracks
            .iter()
            .flatten()
            .map(|b| b.end)
            .max()
            .unwrap_or(0)
    }

    /// The timeline rack (`RACK TYPE="1"`); `key` names it for stable
    /// ids (the bank and scene).
    #[must_use]
    pub fn rack(&self, key: &str) -> Element {
        let ticks = self
            .length()
            .saturating_mul(TICKS_PER_SECOND)
            .div_ceil(1000);
        let bpm = bpm_text(self.bpm);
        let mut timelines = Element::new("TIMELINES");
        // The blocks, then the empty timeline the desk keeps below them.
        let empty = Vec::new();
        for (index, track) in self.tracks.iter().chain([&empty]).enumerate() {
            let mut blocks = Element::new("BLOCKS");
            for (i, b) in track.iter().enumerate() {
                let uid = stable_uid(&format!("block:{key}/{index}/{i}"));
                let el = match &b.content {
                    Content::Audio { path } => Element::new("BLOCK")
                        .with("TYPE", 2)
                        .with("DASUID", uid)
                        .with("NAME", &b.name)
                        .with("START", b.start)
                        .with("END", b.end)
                        .with("POSITION", 0)
                        .with("FADEIN", b.fade_in)
                        .with("FADEOUT", b.fade_out)
                        .with("DASTLMEDIAPATH", path)
                        .with("DASTLHEIGHT", 1)
                        .with("BPM", &bpm)
                        .with("ALLOWLOOP", 1)
                        // At the grid's own tempo, so nothing is stretched.
                        .with("CONFORM_TO_TEMPO", 1),
                    Content::Scene { uid: scene } => Element::new("BLOCK")
                        .with("TYPE", 1)
                        .with("DASUID", uid)
                        .with("NAME", &b.name)
                        .with("START", b.start)
                        .with("END", b.end)
                        .with("POSITION", 0)
                        .with("FADEIN", b.fade_in)
                        .with("FADEOUT", b.fade_out)
                        .with("SPEED", 1)
                        .with("ALLOWLOOP", 1)
                        // A cooked scene's steps are already timed in
                        // seconds; the grid must not stretch them.
                        .with("CONFORM_TO_TEMPO", 0)
                        .with("SCENEUUID", scene),
                };
                blocks.push(el.child(Element::new("DASTLSUBLINES").with("DASTLNB", 0)));
            }
            timelines.push(
                Element::new("TIMELINE")
                    .with("DASUID", stable_uid(&format!("timeline:{key}/{index}")))
                    .with("NAME", index)
                    .with("INDEX", index)
                    .with("DASTLLOCKED", 0)
                    .with("DASTLMUTED", 0)
                    .with("DASTLFOLDED", 1)
                    .child(blocks),
            );
        }
        Element::new("RACK")
            .with("TYPE", 1)
            .with("DURATION", ticks)
            .with("DISPLAY_MODE", 1)
            .with("GRID_BPM", bpm)
            .with("GRID_OFFSET", 0)
            .child(timelines)
    }

    /// The Super Scene as a scene of bank `bank`: no values of its own,
    /// its timeline the only rack.
    #[must_use]
    pub fn scene(&self, bank: &str) -> Scene {
        let mut scene = Scene::new(&self.name, &self.color);
        scene
            .racks
            .push(self.rack(&format!("{bank}/{}", self.name)));
        scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::Patch;

    /// Out Of The Black's reference track, as the recital set warps it.
    fn reference() -> WarpMap {
        WarpMap::new(
            [
                (0.0, 3.625),
                (265.424_541_666_666_64, 707.0),
                (278.047_833_333_311_7, 739.0),
                (290.277_145_833_310_1, 771.0),
                (324.123_312_499_910_7, 859.0),
            ]
            .map(|(seconds, beat)| WarpMarker { seconds, beat })
            .to_vec(),
        )
        .unwrap_or_else(|| WarpMap {
            markers: Vec::new(),
        })
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn beats_land_on_their_markers_and_between_them_in_line() {
        let map = reference();
        assert!(close(map.seconds_at(3.625), 0.0));
        assert!(close(map.seconds_at(707.0), 265.424_541_666_666_64));
        assert!(close(map.seconds_at(859.0), 324.123_312_499_910_7));
        // Halfway between two markers is halfway in time.
        let mid = f64::midpoint(278.047_833_333_311_7, 290.277_145_833_310_1);
        assert!(close(map.seconds_at(755.0), mid));
        // The first stretch is 703.375 beats in 265.4 s: 159.0 bpm.
        let bpm = 703.375 * 60.0 / 265.424_541_666_666_64;
        assert!(close(map.seconds_at(103.625), 100.0 * 60.0 / bpm));
    }

    #[test]
    fn a_beat_before_the_audio_is_its_first_second() {
        let map = reference();
        assert!(close(map.seconds_at(0.0), 0.0));
        assert!(close(map.seconds_at(-4.0), 0.0));
        // Past the last marker the last segment carries on.
        assert!(map.seconds_at(863.0) > 324.123);
    }

    #[test]
    fn a_map_needs_two_distinct_markers() {
        let m = |beat| WarpMarker { seconds: 0.0, beat };
        assert!(WarpMap::new(vec![m(1.0)]).is_none());
        assert!(WarpMap::new(vec![m(1.0), m(1.0)]).is_none());
        assert!(WarpMap::new(vec![m(2.0), m(1.0)]).is_some());
    }

    #[test]
    fn the_timeline_is_written_as_the_desk_saves_one() {
        let show = SuperScene {
            name: "Song".into(),
            color: "#ff855fa8".into(),
            bpm: 159.191,
            tracks: vec![
                vec![Block {
                    name: "Track".into(),
                    content: Content::Audio {
                        path: "/Users/x/Track.wav".into(),
                    },
                    start: 0,
                    end: 240_388,
                    fade_in: 0,
                    fade_out: 0,
                }],
                vec![Block {
                    name: "01 Verse".into(),
                    content: Content::Scene { uid: "abc".into() },
                    start: 12_000,
                    end: 24_000,
                    fade_in: 755,
                    fade_out: 0,
                }],
            ],
        };
        let rack = show.rack("SHOWS/Song");
        assert_eq!(rack.get("TYPE"), Some("1"));
        // 240.388 s is 6009.7 ticks, rounded up as the desk does.
        assert_eq!(rack.get("DURATION"), Some("6010"));
        assert_eq!(rack.get("GRID_BPM"), Some("159.191"));
        let timelines = rack.descendants("TIMELINE");
        assert_eq!(timelines.len(), 3, "two used, one empty below them");
        assert_eq!(timelines.get(2).and_then(|t| t.get("INDEX")), Some("2"));
        let blocks = rack.descendants("BLOCK");
        let audio = blocks.first().copied().cloned().unwrap_or_default();
        assert_eq!(audio.get("TYPE"), Some("2"));
        assert_eq!(audio.get("END"), Some("240388"));
        assert_eq!(audio.get("DASTLMEDIAPATH"), Some("/Users/x/Track.wav"));
        assert_eq!(audio.get("BPM"), Some("159.191"));
        let cue = blocks.get(1).copied().cloned().unwrap_or_default();
        assert_eq!(cue.get("TYPE"), Some("1"));
        assert_eq!(cue.get("SCENEUUID"), Some("abc"));
        assert_eq!(cue.get("FADEIN"), Some("755"));
        assert_eq!(cue.get("CONFORM_TO_TEMPO"), Some("0"));
        assert!(
            cue.to_xml()
                .ends_with("<DASTLSUBLINES DASTLNB=\"0\"/></BLOCK>")
        );

        // As a scene: no values, the timeline its only rack.
        let el = show.scene("SHOWS").to_element("SHOWS", &Patch::default());
        let el = el.unwrap_or_default();
        assert_eq!(el.find("FIXTUREDATAS").and_then(|f| f.get("NB")), Some("0"));
        let racks: Vec<&Element> = el
            .find("RACKS")
            .map(|r| r.elements().collect())
            .unwrap_or_default();
        assert_eq!(
            racks.iter().map(|r| r.get("TYPE")).collect::<Vec<_>>(),
            [Some("1")]
        );
    }

    #[test]
    fn times_are_whole_milliseconds() {
        assert_eq!(millis(1.5094), 1509);
        assert_eq!(millis(-1.0), 0);
        assert_eq!(bpm_text(158.0), "158");
        assert_eq!(bpm_text(158.35), "158.35");
    }
}
