//! A song's show on the desk: a bank with a scene per cue, cooked from
//! Ignition (`ignition-viz`'s `cook_song` example), and a Super Scene that
//! plays the song's audio with each cue's scene from its audio time to the
//! next cue's. Hits (a kick, a snare), when the song has them, are short
//! flash blocks on tracks of their own, one per hit kind, over the cues.
//!
//! Three inputs, all re-read on every run so the showfile follows them:
//!
//! - the recording, `cook_song`'s JSON: `{fps, bpm, song, cues: [{name,
//!   beat, fade_secs, frames}]}`, `beat` counted from the top of the
//!   song's first section;
//! - the audio plan, the venue's `daslight/<song>.audio.json`: where the
//!   audio file is on the desk's computer, its length, and the DAW's warp
//!   markers (`[seconds, beat]`, beats on the clip's own count, which is
//!   the song's less `beat_offset`);
//! - optionally the hits, `<song>.hits.json`: a list (or `{hits: [...]}`)
//!   of `{kind, seconds}` (audio seconds) or `{kind, beat}` (song beats).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use ignition_daslight::cook::{self, find_cycle};
use ignition_daslight::fixups::Rig;
use ignition_daslight::timeline::{Block, Content, SuperScene, WarpMap, WarpMarker, millis};
use ignition_daslight::{Error, Fixture, Patch, Scene, Showfile};

/// The bank the Super Scenes go in, apart from the scenes they play: a
/// bank plays one scene at a time, and a cue starting must not stop the
/// timeline that started it.
pub const SHOWS: &str = "SHOWS";

/// How long a hit's flash block is on the timeline.
const HIT_SECONDS: f64 = 0.15;
/// Frames skipped before looking for a cue's cycle.
const WARMUP_SECONDS: u16 = 2;
const TOLERANCE: u8 = 1;

/// One cue as recorded.
struct Cue {
    name: String,
    beat: f64,
    fade_secs: f64,
    frames: Vec<Vec<u8>>,
}

/// Where the song's audio is and how its beats fall in it.
struct AudioPlan {
    path: String,
    seconds: f64,
    beat_offset: f64,
    warp: WarpMap,
}

impl AudioPlan {
    /// The audio second a song beat plays at.
    fn seconds_at(&self, song_beat: f64) -> f64 {
        self.warp
            .seconds_at(song_beat - self.beat_offset)
            .min(self.seconds)
    }
}

/// One hit: what it is and when, in audio seconds.
struct Hit {
    kind: String,
    seconds: f64,
}

fn read_json(path: &Path) -> Result<serde_json::Value, Error> {
    let text =
        std::fs::read_to_string(path).map_err(|e| Error::Io(path.display().to_string(), e))?;
    serde_json::from_str(&text).map_err(|e| Error::Xml(format!("{}: {e}", path.display())))
}

fn text(v: &serde_json::Value, k: &str) -> String {
    v.get(k)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn number(v: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| v.get(*k).and_then(serde_json::Value::as_f64))
}

fn read_recording(path: &Path) -> Result<(u16, String, bool, Vec<Cue>), Error> {
    let json = read_json(path)?;
    let timeline = json.get("timeline").and_then(serde_json::Value::as_bool).unwrap_or(false);
    let fps = json
        .get("fps")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u16::try_from(n).ok())
        .ok_or_else(|| Error::Patch("song recording has no fps".into()))?;
    let byte = |v: &serde_json::Value| v.as_u64().and_then(|n| u8::try_from(n).ok()).unwrap_or(0);
    let cues = json
        .get("cues")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|c| Cue {
            name: text(c, "name"),
            beat: number(c, &["beat"]).unwrap_or(0.0),
            fade_secs: number(c, &["fade_secs"]).unwrap_or(0.0),
            frames: c
                .get("frames")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_array)
                .map(|f| f.iter().map(byte).collect())
                .collect(),
        })
        .collect();
    Ok((fps, text(&json, "song"), timeline, cues))
}

fn read_audio(path: &Path) -> Result<AudioPlan, Error> {
    let json = read_json(path)?;
    let markers = json
        .get("warp")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            let pair = m.as_array()?;
            Some(WarpMarker {
                seconds: pair.first()?.as_f64()?,
                beat: pair.get(1)?.as_f64()?,
            })
        })
        .collect();
    let warp = WarpMap::new(markers)
        .ok_or_else(|| Error::Patch(format!("{}: needs two warp markers", path.display())))?;
    Ok(AudioPlan {
        path: text(&json, "audio"),
        seconds: number(&json, &["seconds"])
            .ok_or_else(|| Error::Patch(format!("{}: no audio length", path.display())))?,
        beat_offset: number(&json, &["beat_offset"]).unwrap_or(0.0),
        warp,
    })
}

fn read_hits(path: &Path, plan: &AudioPlan) -> Result<Vec<Hit>, Error> {
    let json = read_json(path)?;
    let list = json
        .as_array()
        .or_else(|| json.get("hits").and_then(serde_json::Value::as_array))
        .ok_or_else(|| Error::Patch(format!("{}: no list of hits", path.display())))?;
    Ok(list
        .iter()
        .filter_map(|h| {
            let seconds = number(h, &["seconds", "audio_seconds", "audio_secs", "time", "audio_time"])
                .or_else(|| number(h, &["beat"]).map(|b| plan.seconds_at(b)))?;
            let kind = ["kind", "type", "name"]
                .iter()
                .map(|k| text(h, k))
                .find(|k| !k.is_empty())
                .unwrap_or_else(|| "hit".to_owned());
            Some(Hit { kind, seconds })
        })
        .collect())
}

/// Extra timelines built outside the engine (`"layers": true`): scenes
/// written fixture by fixture (by name; a scene may have steps) and the
/// blocks that play them, each list of blocks one timeline.
struct Layers {
    bank: String,
    scenes: Vec<Scene>,
    tracks: Vec<Vec<Block>>,
    skip: Vec<String>,
}

fn layer_values(json: &serde_json::Value, patch: &Patch) -> Vec<(String, Vec<Option<u8>>)> {
    json.as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, values)| {
            let f = patch.fixtures.iter().find(|f| &f.name == name)?;
            let values = values
                .as_array()?
                .iter()
                .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
                .collect();
            Some((f.uid.clone(), values))
        })
        .collect()
}

fn ticks(v: &serde_json::Value, key: &str, default: u32) -> u32 {
    v.get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(default)
}

fn read_layers(json: &serde_json::Value, patch: &Patch) -> Layers {
    let bank = text(json, "bank");
    let mut scenes = Vec::new();
    for s in json.get("scenes").and_then(serde_json::Value::as_array).into_iter().flatten() {
        let color = text(s, "color");
        let mut scene = Scene::new(&text(s, "name"), if color.is_empty() { "#ffff4020" } else { &color });
        // Layers stack: a hit must not stop the mover move under it.
        scene.release_mode = 0;
        scene.priority = 3;
        if let Some(f) = s.get("fixtures") {
            scene.fixtures = layer_values(f, patch);
        }
        scene.steps = s
            .get("steps")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .map(|st| ignition_daslight::Step {
                wait: ticks(st, "wait", 25),
                fade: ticks(st, "fade", 0),
                fixtures: st.get("fixtures").map(|f| layer_values(f, patch)).unwrap_or_default(),
            })
            .collect();
        scenes.push(scene);
    }
    let uid_of = |name: &str| scenes.iter().find(|s| s.name == name).map(|s| s.uid(&bank));
    let tracks = json
        .get("tracks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|t| {
            t.get("blocks")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|b| {
                    let name = text(b, "scene");
                    let uid = uid_of(&name)?;
                    let start = millis(number(b, &["start"])?);
                    let end = millis(number(b, &["end"])?);
                    (end > start).then(|| Block {
                        name: name.clone(),
                        content: Content::Scene { uid },
                        start,
                        end,
                        fade_in: millis(number(b, &["fade_in"]).unwrap_or(0.0)),
                        fade_out: millis(number(b, &["fade_out"]).unwrap_or(0.0)),
                    })
                })
                .collect()
        })
        .collect();
    let skip = json
        .get("skip_in_base")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    Layers { bank, scenes, tracks, skip }
}

/// A cue's name as the desk shows it: no layer dot, no doubled spaces,
/// no characters its fonts are not trusted with.
fn desk_name(name: &str) -> String {
    let name = name.trim_start_matches('\u{b7}').replace(['\u{b7}', '\u{2014}'], "-");
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every fixture at full white, its position left alone: one hit's flash.
fn flash_values(profile: &str) -> Option<Vec<Option<u8>>> {
    let some = |v: Vec<u8>| v.into_iter().map(Some).collect::<Vec<_>>();
    Some(match profile {
        // Dimmer, strobe off, warm whites, bar RGB, macros off, lamp RGB.
        "UKing/ZQ01334 33CH.ssl2" => some(
            [255, 0]
                .into_iter()
                .chain([255; 9])
                .chain([0; 4])
                .chain([255; 18])
                .collect(),
        ),
        "UKing/U'King 36 LED RGB 7 Channel Par.ssl2" => some(vec![255, 255, 255, 255, 0, 0, 0]),
        "Rockville/Rockville Rockstrip 252 28CH.ssl2" => {
            some([255; 25].into_iter().chain([0; 3]).collect())
        }
        "Solena/Max Bar 28 RGB 24CH.ssl2" => some(vec![255; 24]),
        // X, µX, Y, µY kept; white, open gobo, shutter open, dimmer full.
        "_varied/Mini Gobo Moving Head Light.ssl2" => [None; 4]
            .into_iter()
            .chain(
                [0, 0, 255, 255]
                    .into_iter()
                    .map(Some),
            )
            .chain([None, Some(0), Some(0)])
            .collect(),
        _ => return None,
    })
}

fn flash_scene(name: &str, patch: &Patch) -> Scene {
    let mut scene = Scene::new(name, "#ffffffff");
    for f in patch.fixtures.iter().filter(|f| f.universe == 1) {
        let profile = patch
            .profile(&f.profile)
            .and_then(|p| p.get("SSLNAME"))
            .unwrap_or_default();
        if let Some(values) = flash_values(profile) {
            scene.fixtures.push((f.uid.clone(), values));
        }
    }
    scene
}

/// One cue's frames as a scene: its cycle after a warm-up, every fixture
/// written (a cue is the whole stage, tracked state and all).
fn cue_scene(name: &str, cue: &Cue, fps: u16, rig: &Rig, patch: &Patch) -> (Scene, String) {
    let frames = rig.recoloured(&cue.frames);
    let cycle = find_cycle(&frames, usize::from(WARMUP_SECONDS.saturating_mul(fps)), TOLERANCE);
    let end = cycle.start.saturating_add(cycle.len.max(1));
    let span = frames.get(cycle.start..end).unwrap_or_default();
    let all: Vec<&Fixture> = patch.fixtures.iter().filter(|f| f.universe == 1).collect();
    let motion = cook::motion_of(&cook::coarse(span, patch, 1));
    let scene = cook::scene_from_frames(name, "#ffe04020", span, fps, motion, patch, &all);
    let kind = if cycle.len <= 1 {
        "static".to_owned()
    } else {
        format!(
            "{} steps, {:.1} s{} cycle, {}",
            scene.steps.len().max(1),
            f64::from(u32::try_from(cycle.len).unwrap_or(0)) / f64::from(fps),
            if cycle.exact { "" } else { " fitted" },
            match motion {
                cook::Motion::Snap => "snap",
                cook::Motion::Fade => "fade",
            }
        )
    };
    (scene, kind)
}

/// One cue's frames as they are, start to end, for a song recorded as a
/// whole (`cook_timeline`): every frame a step, runs of identical frames
/// merged into one longer wait, no loop fitting and no fades between
/// steps (the frames already hold every fade).
fn timeline_scene(name: &str, cue: &Cue, fps: u16, rig: &Rig, patch: &Patch, skip: &[String]) -> (Scene, String) {
    let frames = rig.recoloured(&cue.frames);
    let all: Vec<&Fixture> = patch
        .fixtures
        .iter()
        .filter(|f| f.universe == 1 && !skip.iter().any(|p| f.name.starts_with(p.as_str())))
        .collect();
    let timed = cook::steps(&frames, fps, cook::Motion::Snap, frames.len().max(1));
    let slice = |frame: &[u8]| -> Vec<(String, Vec<Option<u8>>)> {
        all.iter()
            .filter_map(|f| Some((f.uid.clone(), cook::fixture_values(frame, patch, f)?)))
            .collect()
    };
    let mut scene = Scene::new(name, "#ffe04020");
    match timed.as_slice() {
        [] => {}
        [one] => scene.fixtures = slice(&one.frame),
        many => {
            scene.steps = many
                .iter()
                .map(|t| ignition_daslight::Step { wait: t.wait, fade: 0, fixtures: slice(&t.frame) })
                .collect();
        }
    }
    let kind = format!("{} steps from {} frames, as played", scene.steps.len().max(1), frames.len());
    (scene, kind)
}

/// `m:ss.mmm`.
fn clock(seconds: f64) -> String {
    let ms = millis(seconds);
    format!(
        "{}:{:02}.{:03}",
        ms / 60_000,
        (ms / 1000) % 60,
        ms % 1000
    )
}

/// Hits as flash scenes, one per kind, and a timeline of short blocks
/// per kind: one at each hit, cut short where the next hit of its kind
/// comes sooner.
fn hit_tracks(hits: &[Hit], bank: &str, audio_end: f64, patch: &Patch) -> (Vec<Scene>, Vec<Vec<Block>>) {
    let mut by_kind: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for h in hits {
        by_kind.entry(h.kind.clone()).or_default().push(h.seconds);
    }
    let mut scenes = Vec::new();
    let mut tracks = Vec::new();
    for (kind, mut times) in by_kind {
        times.sort_by(f64::total_cmp);
        let scene = flash_scene(&format!("HIT {}", kind.to_uppercase()), patch);
        let uid = scene.uid(bank);
        let track: Vec<Block> = times
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let next = times.get(i.saturating_add(1)).copied().unwrap_or(f64::INFINITY);
                let (from, to) = (millis(*t), millis((t + HIT_SECONDS).min(next).min(audio_end)));
                (to > from).then(|| Block {
                    name: scene.name.clone(),
                    content: Content::Scene { uid: uid.clone() },
                    start: from,
                    end: to,
                    fade_in: 0,
                    fade_out: 0,
                })
            })
            .collect();
        tracks.push(track);
        scenes.push(scene);
    }
    (scenes, tracks)
}

/// Adds the song's cue bank, its hit bank (with hits) and its Super Scene
/// (in [`SHOWS`]) to the showfile; the report of what went where.
#[allow(clippy::too_many_lines, reason = "one straight-line build of the song bank, its layers and the Super Scene")]
pub fn add(
    show: &mut Showfile,
    patch: &Patch,
    recording: &Path,
    audio: &Path,
    hits: Option<&Path>,
) -> Result<String, Error> {
    let (fps, song, timeline, cues) = read_recording(recording)?;
    let plan = read_audio(audio)?;
    let rig = Rig::of(patch);
    let bank = song.to_uppercase();
    let extra = hits.map(read_json).transpose()?;
    let layers = extra
        .as_ref()
        .filter(|j| j.get("layers").and_then(serde_json::Value::as_bool) == Some(true))
        .map(|j| read_layers(j, patch));
    let hits = match (&layers, hits) {
        (None, Some(p)) => read_hits(p, &plan)?,
        _ => Vec::new(),
    };
    let skip: Vec<String> = layers.as_ref().map(|l| l.skip.clone()).unwrap_or_default();

    let mut report = format!(
        "# {song}\n\nThe song's show on the desk, generated by `mobile_rig` from Ignition's cue list \
         (cooked cue by cue by `ignition-viz`'s `cook_song`) and the audio plan `{}`. \
         Bank **{bank}** holds a scene per cue; **{SHOWS}** holds the Super Scene **{song}**: \
         the audio on timeline 0, each cue's scene on timeline 1 from its audio time to the next \
         cue's, faded in over the cue's fade{}.\n\n\
         | # | Scene | Song beat | Audio | Until | Fade | Cooked |\n|---|---|---|---|---|---|---|\n",
        audio.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
        if hits.is_empty() { "" } else { ", hits as flash blocks on the timelines below" },
    );
    let mut scenes = Vec::new();
    let mut blocks = Vec::new();
    for (i, cue) in cues.iter().enumerate() {
        let name = format!("{:02} {}", i.saturating_add(1), desk_name(&cue.name));
        let (scene, kind) = if timeline {
            let (mut scene, kind) = timeline_scene(&name, cue, fps, &rig, patch, &skip);
            if !skip.is_empty() {
                // Layers go over these looks: the looks give way.
                scene.priority = 1;
            }
            (scene, kind)
        } else {
            cue_scene(&name, cue, fps, &rig, patch)
        };
        let start = plan.seconds_at(cue.beat);
        let end = cues
            .get(i.saturating_add(1))
            .map_or(plan.seconds, |next| plan.seconds_at(next.beat));
        let (from, to) = (millis(start), millis(end));
        let _ = writeln!(
            report,
            "| {} | {name} | {} | {} | {} | {:.3} s | {kind} |",
            i.saturating_add(1),
            cue.beat,
            clock(start),
            clock(end),
            cue.fade_secs
        );
        if to > from {
            blocks.push(Block {
                name: name.clone(),
                content: Content::Scene {
                    uid: scene.uid(&bank),
                },
                start: from,
                end: to,
                // A whole-song recording already holds each fade.
                fade_in: if timeline { 0 } else { millis(cue.fade_secs).min(to.saturating_sub(from)) },
                fade_out: 0,
            });
        }
        scenes.push(scene);
    }
    show.add_bank(&bank, "#ffe04020", &scenes, patch)?;

    let mut tracks = vec![
        vec![Block {
            name: song.clone(),
            content: Content::Audio {
                path: plan.path.clone(),
            },
            start: 0,
            end: millis(plan.seconds),
            fade_in: 0,
            fade_out: 0,
        }],
        blocks,
    ];

    if !hits.is_empty() {
        let hit_bank = format!("{bank} HITS");
        let (flashes, hit_tracks) = hit_tracks(&hits, &hit_bank, plan.seconds, patch);
        report.push_str("\n## Hits\n\n| Kind | Scene | Hits | Timeline |\n|---|---|---|---|\n");
        for (scene, track) in flashes.iter().zip(&hit_tracks) {
            let _ = writeln!(
                report,
                "| {} | {} | {} | {} |",
                scene.name.trim_start_matches("HIT ").to_lowercase(),
                scene.name,
                track.len(),
                tracks.len()
            );
            tracks.push(track.clone());
        }
        show.add_bank(&hit_bank, "#ffffffff", &flashes, patch)?;
    }

    if let Some(layers) = layers {
        report.push_str("\n## Layers\n\n| Timeline | Blocks |\n|---|---|\n");
        for track in layers.tracks {
            let _ = writeln!(report, "| {} | {} |", tracks.len(), track.len());
            tracks.push(track);
        }
        show.add_bank(&layers.bank, "#ffff4020", &layers.scenes, patch)?;
    }

    let timeline = SuperScene {
        name: song,
        color: "#ff855fa8".to_owned(),
        bpm: plan.warp.mean_bpm(),
        tracks,
    };
    show.add_bank(SHOWS, "#ff855fa8", &[timeline.scene(SHOWS)], patch)?;
    let _ = writeln!(
        report,
        "\nGrid {:.2} bpm (the warp markers' mean); audio `{}`, {}.",
        timeline.bpm,
        plan.path,
        clock(plan.seconds)
    );
    Ok(report)
}
