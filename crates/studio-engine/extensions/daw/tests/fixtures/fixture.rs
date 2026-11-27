//! A fixture session: its folder under `sessions/` (a song file, maybe a
//! chart, a `fixture.toml`), staged into a run folder of its own with the
//! recordings its `fixture.toml` asks for generated beside it. Audio is
//! never committed: every recording is made here, the same every run.
//!
//! `fixture.toml`:
//!
//! ```toml
//! about = "what the session is for"
//!
//! [[media]]                 # a recording to generate
//! file = "Media/bass.wav"   # where, from the session's folder
//! kind = "tone"             # tone (a sine at `hz`, `db` dBFS), click (`bpm`),
//! hz = 55                   # drum (`voices`) or trigger (`piece`)
//! db = -12
//! seconds = 16
//!
//! [[piece]]                 # a drum, for `drum` and `trigger` recordings
//! name = "kick"             # what a voice or a trigger names it by
//! hz = 55                   # its body (a kick's falls from twice this)
//! decay_ms = 160            # its ring, to 1/e
//! noise = 0.1               # how much of it is noise (a snare's wires), 0..1
//! bpm = 120                 # where it is hit: these beats of 4/4 bars
//! beats = [1, 3]
//! bars = 8
//! levels_db = [0, -6]       # each stroke's level, in turn (the drummer)
//!
//! [[media]]                 # a mic on the kit: what of each piece it hears
//! file = "Media/kick-in.wav"
//! kind = "drum"
//! seconds = 16
//! voices = [{ piece = "kick", db = -6 }, { piece = "snare", db = -30, delay_ms = 1.5, invert = false }]
//!
//! [[listen]]                # a track that must be heard at a level
//! track = "Bass"            # as LISTEN names it (a name, a path, a tail)
//! from = "Verse 1"          # bars, `12.5s` or sections, as LISTEN takes them
//! to = "Verse 1 end"
//! lufs_min = -24
//! lufs_max = -10
//!
//! sections = ["Intro", "Verse 1"]   # the song's sections, in order
//! section_ends = [4.0, 12.0]          # and where each ends, seconds
//!
//! [[arrangement]]           # another arrangement: a branch of the song's
//! name = "Big Chorus"       # project file, made over MCP as the test runs
//! insert = { kind = "chorus", from = "Verse 1", to = "Verse 1 end" }  # SECTION_INSERT
//! sections = ["IN", "CH A", "CH B"]
//!
//! [[chart]]                 # where the chart must be at a time
//! at = 4.5                  # seconds
//! source = "chart"          # chart (its .kf, the default) or session
//! bar = 3                   # as played; `written`, `pass`, `section` too
//!
//! [shown]                   # the chart the session shows (SESSION_CHART)
//! source = "generated"      # chart (written: kept as its .kf) or generated
//! sections = [{ name = "Verse 1", first_bar = 1, bars = 4 }]
//! ```

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The folder the fixtures live in.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// One fixture session, as its `fixture.toml` describes it.
#[derive(Debug)]
pub struct Fixture {
    pub name: String,
    pub about: String,
    pub media: Vec<Media>,
    pub listen: Vec<Listen>,
    pub chart: Vec<ChartPoint>,
    /// The song's sections, in order (unchecked when empty).
    pub sections: Vec<String>,
    /// Where each of them ends, seconds (unchecked when empty).
    pub section_ends: Vec<f64>,
    pub arrangements: Vec<Arrangement>,
    /// The chart it must show, when its `fixture.toml` says.
    pub shown: Option<Shown>,
    /// The drums its `drum` and `trigger` recordings play.
    pub pieces: Vec<Piece>,
}

/// The chart a session must show (`SESSION_CHART`): whose it is, and its
/// sections as it numbers their bars.
#[derive(Debug)]
pub struct Shown {
    /// `chart` (written) or `generated` (from the markers).
    pub source: String,
    /// Each section's name, first bar and bars (unchecked when empty).
    pub sections: Vec<(String, i64, i64)>,
}

/// A drum, as a `drum` or `trigger` recording plays it.
#[derive(Debug, Clone)]
pub struct Piece {
    pub name: String,
    pub hz: f64,
    pub decay_ms: f64,
    pub noise: f64,
    /// Each stroke: when (seconds) and how loud (dB, 0 the loudest).
    pub strokes: Vec<(f64, f64)>,
}

/// What of a piece a mic hears.
#[derive(Debug, Clone)]
pub struct Voice {
    pub piece: String,
    pub db: f64,
    pub delay_ms: f64,
    pub invert: bool,
}

/// Another arrangement of the song: a branch of its project file, made by
/// running SECTION_INSERT with `insert` in it.
#[derive(Debug)]
pub struct Arrangement {
    pub name: String,
    pub insert: Value,
    pub sections: Vec<String>,
}

/// A recording to generate.
#[derive(Debug)]
pub struct Media {
    pub file: String,
    pub sound: Sound,
    pub seconds: f64,
}

#[derive(Debug)]
pub enum Sound {
    Tone {
        hz: f64,
        db: f64,
    },
    Click {
        bpm: f64,
    },
    /// A mic on the kit: each of its voices, added.
    Drum {
        voices: Vec<Voice>,
    },
    /// A piece's trigger: one short spike a stroke, at the stroke's level.
    Trigger {
        piece: String,
    },
}

/// A track that must be heard at a level.
#[derive(Debug)]
pub struct Listen {
    pub track: String,
    pub from: String,
    pub to: String,
    pub lufs_min: f64,
    pub lufs_max: f64,
}

/// Where the chart must be at a time: the fields `SESSION_CHART_AT`
/// answers, those given.
#[derive(Debug)]
pub struct ChartPoint {
    pub at: f64,
    /// Each expected field of the answer and its value.
    pub expect: Vec<(String, Value)>,
}

impl Fixture {
    /// The fixture `sessions/<name>`.
    ///
    /// # Errors
    ///
    /// Its `fixture.toml` is missing or says something this does not know.
    pub fn load(name: &str) -> Result<Self, String> {
        let file = root().join("sessions").join(name).join("fixture.toml");
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        Self::parse(name, &text).map_err(|e| format!("{}: {e}", file.display()))
    }

    fn parse(name: &str, text: &str) -> Result<Self, String> {
        let table: toml::Table = text.parse().map_err(|e| format!("{e}"))?;
        let rows = |key: &str| -> Vec<toml::Table> {
            table
                .get(key)
                .and_then(toml::Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(toml::Value::as_table)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        };
        let media = rows("media")
            .iter()
            .map(|m| {
                let sound = match text_of(m, "kind")?.as_str() {
                    "tone" => Sound::Tone {
                        hz: number(m, "hz")?,
                        db: number(m, "db")?,
                    },
                    "click" => Sound::Click {
                        bpm: number(m, "bpm")?,
                    },
                    "drum" => Sound::Drum {
                        voices: m
                            .get("voices")
                            .and_then(toml::Value::as_array)
                            .ok_or("media: a drum's `voices`")?
                            .iter()
                            .map(|v| {
                                let v = v.as_table().ok_or("a voice: a table")?;
                                Ok(Voice {
                                    piece: text_of(v, "piece")?,
                                    db: number(v, "db")?,
                                    delay_ms: number(v, "delay_ms").unwrap_or(0.0),
                                    invert: v
                                        .get("invert")
                                        .and_then(toml::Value::as_bool)
                                        .unwrap_or(false),
                                })
                            })
                            .collect::<Result<_, String>>()?,
                    },
                    "trigger" => Sound::Trigger {
                        piece: text_of(m, "piece")?,
                    },
                    other => {
                        return Err(format!(
                            "media: no kind {other:?} (tone, click, drum, trigger)"
                        ));
                    }
                };
                Ok(Media {
                    file: text_of(m, "file")?,
                    sound,
                    seconds: number(m, "seconds")?,
                })
            })
            .collect::<Result<_, String>>()?;
        let listen = rows("listen")
            .iter()
            .map(|l| {
                Ok(Listen {
                    track: text_of(l, "track")?,
                    from: text_of(l, "from")?,
                    to: text_of(l, "to")?,
                    lufs_min: number(l, "lufs_min")?,
                    lufs_max: number(l, "lufs_max")?,
                })
            })
            .collect::<Result<_, String>>()?;
        let chart = rows("chart")
            .iter()
            .map(|c| {
                let at = number(c, "at")?;
                let mut expect = vec![(
                    "source".to_owned(),
                    Value::String(
                        c.get("source")
                            .and_then(toml::Value::as_str)
                            .unwrap_or("chart")
                            .to_owned(),
                    ),
                )];
                for key in ["bar", "written", "pass"] {
                    if let Some(n) = c.get(key).and_then(toml::Value::as_integer) {
                        expect.push((key.to_owned(), Value::from(n)));
                    }
                }
                if let Some(s) = c.get("section").and_then(toml::Value::as_str) {
                    expect.push(("section".to_owned(), Value::String(s.to_owned())));
                }
                Ok(ChartPoint { at, expect })
            })
            .collect::<Result<_, String>>()?;
        let arrangements = rows("arrangement")
            .iter()
            .map(|a| {
                let insert = a
                    .get("insert")
                    .ok_or("arrangement: `insert`, SECTION_INSERT's arguments")?;
                Ok(Arrangement {
                    name: text_of(a, "name")?,
                    insert: serde_json::to_value(insert).map_err(|e| e.to_string())?,
                    sections: names(a, "sections"),
                })
            })
            .collect::<Result<_, String>>()?;
        let shown = table
            .get("shown")
            .and_then(toml::Value::as_table)
            .map(|t| {
                let sections = t
                    .get("sections")
                    .and_then(toml::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(toml::Value::as_table)
                    .map(|s| {
                        let int = |key: &str| {
                            s.get(key)
                                .and_then(toml::Value::as_integer)
                                .ok_or_else(|| format!("shown.sections: `{key}`"))
                        };
                        Ok((text_of(s, "name")?, int("first_bar")?, int("bars")?))
                    })
                    .collect::<Result<_, String>>()?;
                Ok::<_, String>(Shown {
                    source: text_of(t, "source")?,
                    sections,
                })
            })
            .transpose()?;
        let pieces = rows("piece")
            .iter()
            .map(piece_of)
            .collect::<Result<_, String>>()?;
        Ok(Self {
            shown,
            pieces,
            sections: names(&table, "sections"),
            section_ends: table
                .get("section_ends")
                .and_then(toml::Value::as_array)
                .map(|v| {
                    v.iter()
                        .filter_map(|n| {
                            n.as_float().or_else(|| {
                                n.as_integer()
                                    .and_then(|i| i32::try_from(i).ok())
                                    .map(f64::from)
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            arrangements,
            name: name.to_owned(),
            about: table
                .get("about")
                .and_then(toml::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            media,
            listen,
            chart,
        })
    }

    /// Copy the session into `run/sessions/<name>/` and generate its
    /// recordings there. Returns its song file.
    ///
    /// # Errors
    ///
    /// A file could not be copied or written, or the folder has no song.
    pub fn stage(&self, run: &Path) -> Result<PathBuf, String> {
        let from = root().join("sessions").join(&self.name);
        let to = run.join("sessions").join(&self.name);
        copy_dir(&from, &to)?;
        generate(&to, &self.media, &self.pieces)?;
        song_in(&to)
    }
}

impl Fixture {
    /// The chart written beside the session (its one `.kf`), when it has one.
    #[must_use]
    pub fn written_chart(&self) -> Option<String> {
        let dir = root().join("sessions").join(&self.name);
        let mut charts = std::fs::read_dir(&dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "kf"));
        let one = charts.next()?;
        // Two charts are none, as the engine finds a song's chart.
        charts
            .next()
            .is_none()
            .then(|| std::fs::read_to_string(one).ok())
            .flatten()
    }
}

/// Write each of `media` under `dir`, at its `file`.
///
/// # Errors
///
/// A folder could not be made or a file written.
pub fn generate(dir: &Path, media: &[Media], pieces: &[Piece]) -> Result<(), String> {
    for media in media {
        let path = dir.join(&media.file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        write_wav(&path, &render(&media.sound, media.seconds, pieces)?)?;
    }
    Ok(())
}

/// Every fixture session, by folder name.
pub fn all() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root().join("sessions"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().join("fixture.toml").is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// The song file in `dir`: its `.RPP` or `.session`.
fn song_in(dir: &Path) -> Result<PathBuf, String> {
    std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("rpp") || x == "session")
        })
        .ok_or_else(|| format!("no .RPP or .session in {}", dir.display()))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if entry.file_name() != "fixture.toml" {
            std::fs::copy(entry.path(), &target)
                .map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    Ok(())
}

fn text_of(t: &toml::Table, key: &str) -> Result<String, String> {
    t.get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("`{key}`: a string"))
}

fn names(t: &toml::Table, key: &str) -> Vec<String> {
    t.get(key)
        .and_then(toml::Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// A `[[piece]]`: its sound and its strokes, beat by beat.
fn piece_of(p: &toml::Table) -> Result<Piece, String> {
    let bpm = number(p, "bpm")?;
    let bars = number(p, "bars")?;
    let beats = p
        .get("beats")
        .and_then(toml::Value::as_array)
        .ok_or("piece: `beats`, the beats of a bar it is hit on")?
        .iter()
        .map(|b| b.as_float().or_else(|| b.as_integer().map(|i| i as f64)))
        .collect::<Option<Vec<f64>>>()
        .ok_or("piece: `beats` are numbers")?;
    let levels: Vec<f64> = match p.get("levels_db").and_then(toml::Value::as_array) {
        None => vec![0.0],
        Some(l) => l
            .iter()
            .map(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
            .collect::<Option<Vec<f64>>>()
            .filter(|l| !l.is_empty())
            .ok_or("piece: `levels_db`, one or more numbers")?,
    };
    let beat = 60.0 / bpm;
    let strokes = (0..bars as usize)
        .flat_map(|bar| {
            beats
                .iter()
                .map(move |b| (bar as f64 * 4.0 + b - 1.0) * beat)
        })
        .zip(levels.iter().copied().cycle())
        .collect();
    Ok(Piece {
        name: text_of(p, "name")?,
        hz: number(p, "hz")?,
        decay_ms: number(p, "decay_ms")?,
        noise: number(p, "noise").unwrap_or(0.0),
        strokes,
    })
}

fn number(t: &toml::Table, key: &str) -> Result<f64, String> {
    match t.get(key) {
        Some(toml::Value::Float(f)) => Ok(*f),
        #[expect(clippy::cast_precision_loss, reason = "a small number in a fixture")]
        Some(toml::Value::Integer(i)) => Ok(*i as f64),
        _ => Err(format!("`{key}`: a number")),
    }
}

const RATE: u32 = 48_000;

/// The samples of `sound` over `seconds`.
fn render(sound: &Sound, seconds: f64, pieces: &[Piece]) -> Result<Vec<f32>, String> {
    let n = (seconds * f64::from(RATE)) as usize;
    let piece = |name: &str| {
        pieces
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| format!("no [[piece]] {name:?}"))
    };
    match sound {
        Sound::Drum { voices } => {
            let mut out = vec![0.0_f64; n];
            for v in voices {
                let p = piece(&v.piece)?;
                let gain = 10_f64.powf(v.db / 20.0) * if v.invert { -1.0 } else { 1.0 };
                for &(at, level_db) in &p.strokes {
                    let start = ((at + v.delay_ms / 1e3) * f64::from(RATE)).round() as usize;
                    let stroke = gain * 10_f64.powf(level_db / 20.0);
                    let mut noise = Noise::new(start as u64 + p.name.len() as u64);
                    for (i, s) in out.iter_mut().skip(start).enumerate() {
                        let t = i as f64 / f64::from(RATE);
                        if t > p.decay_ms * 8.0 / 1e3 {
                            break;
                        }
                        *s += stroke * hit(p, t, &mut noise);
                    }
                }
            }
            Ok(out.into_iter().map(|s| s as f32).collect())
        }
        Sound::Trigger { piece: name } => {
            let p = piece(name)?;
            let mut out = vec![0.0_f32; n];
            for &(at, level_db) in &p.strokes {
                let start = (at * f64::from(RATE)).round() as usize;
                let peak = 0.9 * 10_f64.powf(level_db / 20.0);
                // A piezo's spike: up in a sample, gone in a few ms.
                for (i, s) in out
                    .iter_mut()
                    .skip(start)
                    .take(RATE as usize / 200)
                    .enumerate()
                {
                    *s = (peak * (-(i as f64) / 24.0).exp()) as f32;
                }
            }
            Ok(out)
        }
        Sound::Tone { .. } | Sound::Click { .. } => Ok(steady(sound, n)),
    }
}

/// One stroke of `p`, `t` seconds after it: its body (a sine falling to
/// its pitch, as a struck head does), a click on the attack, and its
/// noise.
fn hit(p: &Piece, t: f64, noise: &mut Noise) -> f64 {
    let ring = (-t * 1e3 / p.decay_ms).exp();
    // The pitch falls from twice the body to it over the first 20 ms; the
    // phase is the frequency's integral.
    let fall = 0.02;
    let phase = p.hz * (t + fall * (1.0 - (-t / fall).exp()));
    let body = (std::f64::consts::TAU * phase).sin();
    let click = (-t * 1e3 / 1.5).exp() * noise.next();
    let wires = (-t * 1e3 / (p.decay_ms * 0.8)).exp() * noise.next();
    ring * body * (1.0 - p.noise) + 0.3 * click + p.noise * wires
}

/// A small, seeded noise: the same samples every run.
struct Noise(u64);

impl Noise {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1))
    }

    /// The next sample, -1..1.
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
    }
}

/// A tone's or a click's samples, `n` of them.
fn steady(sound: &Sound, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            let s = match sound {
                Sound::Tone { hz, db } => {
                    10_f64.powf(db / 20.0) * (std::f64::consts::TAU * hz * t).sin()
                }
                // A 20 ms blip on every beat, higher and louder on the
                // downbeat (4/4), as a click track is.
                Sound::Click { bpm } => {
                    let beat = 60.0 / bpm;
                    let k = (t / beat).floor();
                    let into = t - k * beat;
                    let (hz, gain) = if (k as i64) % 4 == 0 {
                        (1_500.0, 0.7)
                    } else {
                        (1_000.0, 0.45)
                    };
                    if into < 0.02 {
                        gain * (-into * 200.0).exp() * (std::f64::consts::TAU * hz * into).sin()
                    } else {
                        0.0
                    }
                }
                Sound::Drum { .. } | Sound::Trigger { .. } => 0.0,
            };
            s as f32
        })
        .collect()
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for &s in samples {
        wav.write_sample(s).map_err(|e| e.to_string())?;
    }
    wav.finalize().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_parses() {
        let names = all();
        assert!(names.len() >= 4, "{names:?}");
        for name in names {
            Fixture::load(&name).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn a_chart_point_expects_only_what_it_names() {
        let f = Fixture::parse("x", "[[chart]]\nat = 1.5\nbar = 2\n").unwrap();
        assert_eq!(
            f.chart[0].expect,
            vec![
                ("source".to_owned(), Value::from("chart")),
                ("bar".to_owned(), Value::from(2)),
            ]
        );
    }
}
