//! A new session, made three ways over MCP (studio#694): from scratch
//! (a title, key, tempo and form), from a folder of stems (a track per
//! file), and from a REAPER project (its tracks, items, markers and tempo
//! kept). Each is made with `SESSION_NEW`, read back over the same door,
//! played, and opened again from the `.session` it wrote.

use std::path::Path;

use serde_json::{Value, json};

use crate::check::{self, Checks};
use crate::fixture::{self, Media, Sound};
use crate::mcp::Mcp;
use crate::{engine, finish, run_dir};

/// `SESSION_NEW` with `args`, then wait for the song it makes to be open.
fn make(engine: &crate::engine::Engine, args: &Value) -> Result<Vec<Value>, String> {
    engine.mcp.run("SESSION_NEW", args)?;
    check::opened(engine)
}

/// The current song's `SONG_INFO`.
fn info(mcp: &Mcp) -> Result<Value, String> {
    mcp.run("SONG_INFO", &json!({}))
}

/// Each section's name and length in seconds, in order.
fn section_spans(mcp: &Mcp) -> Result<Vec<(String, f64)>, String> {
    Ok(mcp
        .run("SESSION_SECTIONS", &json!({}))?
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            let length = s["end"].as_f64().unwrap_or(0.0) - s["start"].as_f64().unwrap_or(0.0);
            (s["name"].as_str().unwrap_or_default().to_owned(), length)
        })
        .collect())
}

/// Every track with its name and the files its items play.
fn tracks_and_sources(mcp: &Mcp) -> Result<Vec<(String, Vec<String>)>, String> {
    let tracks = mcp.run("SESSION_TRACKS", &json!({}))?;
    let mut out = Vec::new();
    for t in tracks.as_array().into_iter().flatten() {
        let name = t["name"].as_str().unwrap_or_default().to_owned();
        let guid = t["guid"].as_str().unwrap_or_default();
        let items = mcp.run("TRACK_ITEMS", &json!({"track": guid}))?;
        let files = items
            .as_array()
            .or_else(|| items["items"].as_array())
            .into_iter()
            .flatten()
            .map(|i| {
                let file = i["source"]
                    .as_str()
                    .or_else(|| i["file"].as_str())
                    .unwrap_or_default();
                Path::new(file)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect();
        out.push((name, files));
    }
    Ok(out)
}

/// Whether `a` and `b` are the same within `by`.
fn near(a: f64, b: f64, by: f64) -> bool {
    (a - b).abs() <= by
}

/// What a song made from scratch must be: its title, tempo, key and form.
/// 96 bpm in 4/4 is 2.5 s a bar.
fn scratch_holds(mcp: &Mcp, when: &str, checks: &mut Checks) -> Result<(), String> {
    let song = info(mcp)?;
    checks.add(
        format!("{when}: title"),
        song["title"] == json!("Morning Song"),
        format!("{}", song["title"]),
    );
    checks.add(
        format!("{when}: tempo"),
        song["bpm"].as_f64().is_some_and(|b| near(b, 96.0, 0.01)),
        format!("{} bpm", song["bpm"]),
    );
    checks.add(
        format!("{when}: key"),
        song["key"] == json!("Eb major"),
        format!("{}", song["key"]),
    );
    let spans = section_spans(mcp)?;
    let bars: Vec<f64> = spans.iter().map(|(_, s)| s / 2.5).collect();
    checks.add(
        format!("{when}: form"),
        bars.len() == 3
            && near(bars[0], 4.0, 0.01)
            && near(bars[1], 8.0, 0.01)
            && near(bars[2], 8.0, 0.01),
        format!("{spans:?}"),
    );
    Ok(())
}

#[test]
fn a_new_session_from_scratch() {
    let run = run_dir("new-scratch");
    let file = run.join("Morning Song.session");
    let (engine, _spans) = engine("new-scratch");
    let mcp = &engine.mcp;
    let mut checks = Checks::default();
    let mut steps = || -> Result<(), String> {
        let songs = make(
            &engine,
            &json!({
                "title": "Morning Song",
                "key": "Eb",
                "tempo": 96,
                "form": ["Intro 4", "Verse 8", "Chorus"],
                "to": file.display().to_string(),
            }),
        )?;
        checks.add("made", songs.len() == 1, format!("{} song", songs.len()));
        scratch_holds(mcp, "made", &mut checks)?;
        match check::plays(mcp, 0.0) {
            Ok(said) => checks.add("plays", true, said),
            Err(why) => checks.failed("plays", why),
        }
        checks.add("saved", file.is_file(), file.display().to_string());
        check::open(&engine, &file)?;
        scratch_holds(mcp, "opened again", &mut checks)
    };
    if let Err(why) = steps() {
        checks.failed("from scratch", why);
    }
    finish(
        "new-scratch",
        "a new session from a title, key, tempo and form",
        engine,
        &checks,
    );
}

/// A folder of stems as a band hands one over: loose files and a folder
/// inside, 8 s long.
fn stems() -> Vec<Media> {
    let tone = |file: &str, hz: f64, db: f64| Media {
        file: file.to_owned(),
        sound: Sound::Tone { hz, db },
        seconds: 8.0,
    };
    vec![
        tone("Bass.wav", 55.0, -12.0),
        tone("Keys.wav", 440.0, -18.0),
        Media {
            file: "Drums/Click.wav".to_owned(),
            sound: Sound::Click { bpm: 120.0 },
            seconds: 8.0,
        },
    ]
}

#[test]
fn a_new_session_from_a_folder_of_stems() {
    let run = run_dir("new-folder");
    let folder = run.join("Night Drive");
    let (engine, _spans) = engine("new-folder");
    let mcp = &engine.mcp;
    let mut checks = Checks::default();
    let mut steps = || -> Result<(), String> {
        fixture::generate(&folder, &stems())?;
        make(
            &engine,
            &json!({"from": folder.display().to_string(), "tempo": 120}),
        )?;
        let song = info(mcp)?;
        checks.add(
            "title",
            song["title"] == json!("Night Drive"),
            format!("{}", song["title"]),
        );
        checks.add(
            "tempo",
            song["bpm"].as_f64().is_some_and(|b| near(b, 120.0, 0.01)),
            format!("{} bpm", song["bpm"]),
        );
        // One track per file, playing it and nothing else (the engine may
        // rename one: its own guide's track is `Click`).
        let tracks = tracks_and_sources(mcp)?;
        let files = ["Bass.wav", "Click.wav", "Keys.wav"];
        for file in files {
            let on: Vec<&(String, Vec<String>)> = tracks
                .iter()
                .filter(|(_, played)| played.iter().any(|f| f == file))
                .collect();
            checks.add(
                format!("track for {file}"),
                on.len() == 1 && on[0].1.iter().all(|f| f == file),
                format!("{on:?}"),
            );
        }
        let stem_tracks = tracks
            .iter()
            .filter(|(_, played)| played.iter().any(|f| files.contains(&f.as_str())))
            .count();
        checks.add(
            "a track per file",
            stem_tracks == files.len(),
            format!("{stem_tracks} tracks play the {} files", files.len()),
        );
        match check::lufs(mcp, "Bass", "0.5s", "7.5s") {
            Ok(Some(v)) => checks.add(
                "heard Bass",
                (-24.0..=-10.0).contains(&v),
                format!("{v:.1} LUFS"),
            ),
            Ok(None) => checks.failed("heard Bass", "silence"),
            Err(why) => checks.failed("heard Bass", why),
        }
        match check::plays(mcp, 0.0) {
            Ok(said) => checks.add("plays", true, said),
            Err(why) => checks.failed("plays", why),
        }
        let file = folder.join("Night Drive.session");
        checks.add("saved", file.is_file(), file.display().to_string());
        check::open(&engine, &file)?;
        let again = tracks_and_sources(mcp)?;
        let stem_tracks = again
            .iter()
            .filter(|(_, played)| played.iter().any(|f| files.contains(&f.as_str())))
            .count();
        checks.add(
            "opened again: a track per file",
            stem_tracks == files.len(),
            format!("{again:?}"),
        );
        match check::plays(mcp, 0.0) {
            Ok(said) => checks.add("opened again: plays", true, said),
            Err(why) => checks.failed("opened again: plays", why),
        }
        Ok(())
    };
    if let Err(why) = steps() {
        checks.failed("from a folder", why);
    }
    finish(
        "new-folder",
        "a new session from a folder of stems",
        engine,
        &checks,
    );
}

/// What `Waltz.RPP` holds, as the session must keep it: 100 bpm in 3/4,
/// Piano with one item and Vox with two, the Verse and Chorus sections
/// and the marker Hit at 5.4 s.
fn waltz_holds(mcp: &Mcp, when: &str, checks: &mut Checks) -> Result<(), String> {
    let song = info(mcp)?;
    checks.add(
        format!("{when}: tempo"),
        song["bpm"].as_f64().is_some_and(|b| near(b, 100.0, 0.01)) && song["meter"] == json!("3/4"),
        format!("{} bpm {}", song["bpm"], song["meter"]),
    );
    let hit = song["markers"].as_array().into_iter().flatten().any(|m| {
        m["name"] == json!("Hit") && m["at"].as_f64().is_some_and(|at| near(at, 5.4, 0.001))
    });
    checks.add(
        format!("{when}: markers"),
        hit,
        format!("{}", song["markers"]),
    );
    let names: Vec<String> = section_spans(mcp)?.into_iter().map(|(n, _)| n).collect();
    checks.add(
        format!("{when}: sections"),
        names == ["Verse", "Chorus"],
        format!("{names:?}"),
    );
    let tracks = tracks_and_sources(mcp)?;
    let items = |name: &str| {
        tracks
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, files)| files.clone())
    };
    checks.add(
        format!("{when}: tracks and items"),
        items("Piano") == Some(vec!["piano.wav".to_owned()])
            && items("Vox") == Some(vec!["vox.wav".to_owned(), "vox.wav".to_owned()]),
        format!("Piano {:?}, Vox {:?}", items("Piano"), items("Vox")),
    );
    Ok(())
}

#[test]
fn a_new_session_from_a_reaper_project() {
    let run = run_dir("new-rpp");
    let dir = run.join("Waltz");
    let rpp = dir.join("Waltz.RPP");
    let (engine, _spans) = engine("new-rpp");
    let mcp = &engine.mcp;
    let mut checks = Checks::default();
    let mut steps = || -> Result<(), String> {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::copy(fixture::root().join("new/from-rpp/Waltz.RPP"), &rpp)
            .map_err(|e| e.to_string())?;
        let tone = |file: &str, hz: f64| Media {
            file: file.to_owned(),
            sound: Sound::Tone { hz, db: -18.0 },
            seconds: 7.2,
        };
        fixture::generate(
            &dir,
            &[tone("Media/piano.wav", 262.0), tone("Media/vox.wav", 330.0)],
        )?;
        make(&engine, &json!({"from": rpp.display().to_string()}))?;
        waltz_holds(mcp, "made", &mut checks)?;
        match check::plays(mcp, 0.0) {
            Ok(said) => checks.add("plays", true, said),
            Err(why) => checks.failed("plays", why),
        }
        let file = dir.join("Waltz.session");
        checks.add("saved", file.is_file(), file.display().to_string());
        // Made once: a second time is refused, the session named.
        let again = mcp.run("SESSION_NEW", &json!({"from": rpp.display().to_string()}));
        checks.add(
            "made once",
            again.as_ref().is_err_and(|e| e.contains("Waltz.session")),
            format!("{again:?}"),
        );
        check::open(&engine, &file)?;
        waltz_holds(mcp, "opened again", &mut checks)
    };
    if let Err(why) = steps() {
        checks.failed("from an RPP", why);
    }
    finish(
        "new-rpp",
        "a new session from a REAPER project",
        engine,
        &checks,
    );
}
