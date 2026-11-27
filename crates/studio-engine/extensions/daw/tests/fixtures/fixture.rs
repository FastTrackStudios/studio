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
//! kind = "tone"             # tone (a sine at `hz`, `db` dBFS) or click (`bpm`)
//! hz = 55
//! db = -12
//! seconds = 16
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
    Tone { hz: f64, db: f64 },
    Click { bpm: f64 },
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
                    other => return Err(format!("media: no kind {other:?} (tone, click)")),
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
        Ok(Self {
            shown,
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
        for media in &self.media {
            let path = to.join(&media.file);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            write_wav(&path, &render(&media.sound, media.seconds))?;
        }
        song_in(&to)
    }
}

impl Fixture {
    /// The chart written beside the session (its `.kf`), when it has one.
    #[must_use]
    pub fn written_chart(&self) -> Option<String> {
        let dir = root().join("sessions").join(&self.name);
        std::fs::read_dir(&dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "kf"))
            .and_then(|p| std::fs::read_to_string(p).ok())
    }
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
fn render(sound: &Sound, seconds: f64) -> Vec<f32> {
    let n = (seconds * f64::from(RATE)) as usize;
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
