//! What a song holds, as JSON: its tracks (the tree, what is muted, how
//! many items each has), its tempo changes and meters, its regions and
//! markers with their lanes, and where bar 1 of its first section is.
//!
//! ```sh
//! song-report "Song/Song.session" ["Other/Other.RPP" …]
//! ```
//!
//! A project file or an `.RPP` is read as it is: nothing is prepared and
//! nothing is written (an `.RPP` gets no `.session` from this). One JSON
//! object a song, a line each.
//!
//! ```sh
//! song-report --levels "Song/Song.session"
//! ```
//!
//! `--levels` also renders ten seconds from a third of the way in, twice
//! (the song heard by its reference, then by its stems, the guide left
//! out both times), and says how loud each is and how far apart they are
//! (`levels`: RMS in dB, and of their difference). A record and its
//! separation should come out a few dB apart at most; a reference or
//! stems that do not play read as silence. Nothing is saved.

use std::path::{Path, PathBuf};

use daw::service::{Items as _, Markers, ProjectContext, Regions, TempoMap as _, Tracks};
use daw::standalone::Standalone;
use serde_json::{Value, json};

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let levels = args.iter().any(|a| a.as_os_str() == "--levels");
    let songs: Vec<&PathBuf> = args
        .iter()
        .filter(|a| a.as_os_str() != "--levels")
        .collect();
    if songs.is_empty() {
        eyre::bail!("usage: song-report [--levels] <song folder | .session | .RPP> …");
    }
    for song in songs {
        println!("{}", report(song, levels)?);
    }
    Ok(())
}

/// The sample rate the levels are rendered at.
const RATE: u32 = 48_000;

/// Ten seconds of `project` from `from` seconds, as it stands, mono.
fn render(daw: &Standalone, project: &str, from: f64) -> Vec<f32> {
    let renderer = daw::standalone::audio_engine::render::ProjectRenderer::new(daw, project, RATE);
    let start = (from * f64::from(RATE)) as u64;
    let mut out = Vec::new();
    let mut at = 0u64;
    let frames = u64::from(RATE) * 10;
    while at < frames {
        let block = renderer.render_block(start + at, 8_192);
        out.extend(block.samples.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5));
        at += 8_192;
    }
    out
}

/// RMS of `samples`, dB.
fn db(samples: impl Iterator<Item = f32>) -> f64 {
    let (sum, count) = samples.fold((0.0_f64, 0_u32), |(sum, count), s| {
        (f64::from(s).mul_add(f64::from(s), sum), count + 1)
    });
    10.0 * (sum / f64::from(count.max(1))).max(1e-12).log10()
}

/// How loud the song is by its reference and by its stems, and how far
/// apart the two are.
fn levels(daw: &Standalone, project: &str, length: f64) -> eyre::Result<Value> {
    let context = ProjectContext::Project(project.to_owned());
    for guid in studio_engine::reference::left_out(daw, project) {
        let _ = Tracks::set_muted(
            daw,
            context.clone(),
            daw::service::TrackRef::Guid(guid),
            true,
        );
    }
    let from = length / 3.0;
    let mut heard = Vec::new();
    for reference in [true, false] {
        studio_engine::instrument_tracks::hear(daw, &context, reference)
            .map_err(|e| eyre::eyre!(e))?;
        heard.push(render(daw, project, from));
    }
    let (by_reference, by_stems) = (&heard[0], &heard[1]);
    Ok(json!({
        "from": from,
        "reference_db": db(by_reference.iter().copied()),
        "stems_db": db(by_stems.iter().copied()),
        "difference_db": db(by_reference.iter().zip(by_stems).map(|(a, b)| a - b)),
    }))
}

/// The song at `path` (a folder's `.session`, else its `.RPP`), read and
/// described.
fn report(path: &Path, with_levels: bool) -> eyre::Result<Value> {
    let file = if path.is_dir() {
        studio_engine::setlist::song_in(path)
            .ok_or_else(|| eyre::eyre!("no song in {}", path.display()))?
    } else {
        path.to_path_buf()
    };
    let daw = Standalone::new();
    let opened = studio_engine::open_core::load_into_with(
        &daw,
        &file,
        if with_levels {
            studio_engine::open_core::Media::All
        } else {
            studio_engine::open_core::Media::Deferred
        },
    )?;
    let project = ProjectContext::Project(opened.project_guid.clone());

    let tracks = Tracks::all(&daw, project.clone());
    let items = daw.get_all_items(project.clone());
    let path_of = |guid: &str| {
        let mut names = Vec::new();
        let mut at = tracks.iter().find(|t| t.guid == guid);
        while let Some(track) = at {
            names.push(track.name.clone());
            at = track
                .parent_guid
                .as_deref()
                .and_then(|p| tracks.iter().find(|t| t.guid == p));
        }
        names.reverse();
        names.join("/")
    };
    let track_rows: Vec<Value> = tracks
        .iter()
        .map(|t| {
            json!({
                "path": path_of(&t.guid),
                "muted": t.muted,
                "folder": t.is_folder,
                "items": items.iter().filter(|i| i.track_guid == t.guid).count(),
            })
        })
        .collect();

    let tempo = daw.get_tempo_points(project.clone());
    let meters: Vec<Value> = tempo
        .iter()
        .filter_map(|p| {
            let sig = p.time_signature.as_ref()?;
            Some(json!({
                "at": p.position.seconds(),
                "meter": format!("{}/{}", sig.numerator, sig.denominator),
            }))
        })
        .collect();
    let bpm = |pick: fn(f64, f64) -> f64| tempo.iter().map(|p| p.bpm).reduce(pick);

    let mut regions = Regions::all(&daw, project.clone());
    regions.sort_by(|a, b| {
        a.time_range
            .start_seconds()
            .total_cmp(&b.time_range.start_seconds())
    });
    let region_rows: Vec<Value> = regions
        .iter()
        .map(|r| {
            let (start, end) = (r.time_range.start_seconds(), r.time_range.end_seconds());
            let (bar, beat, _) = daw.time_to_musical(project.clone(), start);
            json!({
                "name": r.name,
                "lane": r.lane,
                "start": start,
                "end": end,
                "bar": bar,
                "beat": beat,
            })
        })
        .collect();
    let marker_rows: Vec<Value> = Markers::all(&daw, project.clone())
        .iter()
        .map(|m| json!({ "name": m.name, "lane": m.lane, "at": m.position.seconds() }))
        .collect();

    let length = items
        .iter()
        .map(|i| i.position.as_seconds() + i.length.as_seconds())
        .fold(0.0, f64::max);
    let levels = if with_levels {
        levels(&daw, &opened.project_guid, length)?
    } else {
        Value::Null
    };
    Ok(json!({
        "levels": levels,
        "song": file.display().to_string(),
        "name": opened.name,
        "tracks": track_rows.len(),
        "top": tracks.iter().filter(|t| t.parent_guid.is_none()).map(|t| t.name.clone()).collect::<Vec<_>>(),
        "track_list": track_rows,
        "items": items.len(),
        "tempo_points": tempo.len(),
        "bpm_min": bpm(f64::min),
        "bpm_max": bpm(f64::max),
        "meters": meters,
        "regions": region_rows,
        "markers": marker_rows,
    }))
}
