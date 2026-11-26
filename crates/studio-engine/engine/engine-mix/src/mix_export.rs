//! A prepared song written back out as a REAPER project, to finish the
//! mix in REAPER.
//!
//! The engine writes its own project as RPP text, carrying every track's
//! original FX chain over from the file it was opened from — so the
//! plugins a session came with come back as they were. Tracks the
//! session made itself (the buses, the drum Process folder) have no
//! chain there: they get the golden template's, as REAPER's stock
//! plugins with their settings (`dawfile_reaper::stock_fx`), spliced in.
//!
//! The project goes beside the one it was opened from, so the media
//! paths — relative to that folder — still find the audio.

#![cfg(feature = "native")]

use daw::service::{ProjectContext, Projects as _, Tracks};
use dawfile_reaper::builder::ReaperProjectBuilder;
use dawfile_reaper::stock_fx::{ReaComp, ReaEq, ReaVerbate};

/// What [`write_rpp`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exported {
    pub path: std::path::PathBuf,
    /// The tracks given a chain, by name.
    pub chained: Vec<String>,
}

/// The golden template's chain for a track the session made, as one
/// REAPER `<FXCHAIN>` block. `None` for a track that gets none.
#[must_use]
pub fn chain_for(name: &str) -> Option<String> {
    let comp = |threshold: f64, ratio: f64, attack: f64, release: f64| {
        ReaComp::new()
            .threshold_db(threshold)
            .ratio(ratio)
            .attack_ms(attack)
            .release_ms(release)
            .auto_makeup()
    };
    let room = |size: f64, damping: f64| {
        ReaVerbate::new()
            .room_size(size)
            .damping(damping)
            .wet(1.0)
            .dry(0.0)
    };
    let text = match name.trim() {
        // Parallel compressors, dry to crushed.
        "Dry" => one_track(name, |t| t.stock_fx(comp(-6.0, 1.5, 20.0, 150.0))),
        "Tight" => one_track(name, |t| t.stock_fx(comp(-20.0, 4.0, 1.0, 50.0))),
        "Punch" => one_track(name, |t| t.stock_fx(comp(-20.0, 4.0, 15.0, 80.0))),
        "Smash" => one_track(name, |t| t.stock_fx(comp(-35.0, 10.0, 0.5, 100.0))),
        "Crunch" => one_track(name, |t| t.stock_fx(comp(-45.0, 20.0, 0.1, 60.0))),
        // The rooms, small to vast.
        "Room Sim" => one_track(name, |t| t.stock_fx(room(0.15, 0.7))),
        "Wood Room" => one_track(name, |t| t.stock_fx(room(0.3, 0.6))),
        "Music Club" => one_track(name, |t| t.stock_fx(room(0.45, 0.5))),
        "Stadium" => one_track(name, |t| t.stock_fx(room(0.95, 0.3))),
        "RMX 16" => one_track(name, |t| t.stock_fx(room(0.7, 0.4))),
        "Nonlin" => one_track(name, |t| t.stock_fx(room(0.6, 0.8))),
        "Brick Wall" => one_track(name, |t| t.stock_fx(room(0.5, 0.2))),
        // Every bus: EQ, then a gentle compressor (the template's chain).
        bus if dynamic_template::apply::is_bus_track(bus) => one_track(name, |t| {
            t.stock_fx(ReaEq::new())
                .stock_fx(comp(-12.0, 2.0, 10.0, 100.0))
        }),
        _ => return None,
    };
    fx_block(&text)
}

/// A one-track project's RPP text: the track `name` as `fx` builds it.
fn one_track(
    name: &str,
    fx: impl FnOnce(dawfile_reaper::builder::TrackBuilder) -> dawfile_reaper::builder::TrackBuilder,
) -> String {
    use dawfile_reaper::types::RppSerialize as _;
    ReaperProjectBuilder::new()
        .track(name, fx)
        .build()
        .to_rpp_string()
}

/// How far a line is indented.
fn indent(line: &str) -> usize {
    line.len().saturating_sub(line.trim_start().len())
}

/// The line closing the block opened at `open`: the next bare `>` at
/// its indent.
fn close_of(lines: &[&str], open: usize) -> Option<usize> {
    let at = indent(lines.get(open)?);
    lines
        .iter()
        .enumerate()
        .skip(open.saturating_add(1))
        .find(|(_, l)| l.trim() == ">" && indent(l) == at)
        .map(|(i, _)| i)
}

/// The `<FXCHAIN … >` block of a one-track project's text, its indent
/// taken off.
fn fx_block(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim_start().starts_with("<FXCHAIN"))?;
    let end = close_of(&lines, start)?;
    let at = indent(lines.get(start)?);
    Some(
        lines
            .get(start..=end)?
            .iter()
            .map(|l| l.get(at..).unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// `text` with `block` added to the track whose `<TRACK` line names
/// `guid`, when it has no FX chain yet: before its first item, else
/// before its closing line. `None` when it already has one, or no such
/// track.
fn splice(text: &str, guid: &str, block: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let open = lines
        .iter()
        .position(|l| l.trim_start().starts_with("<TRACK") && l.contains(guid))?;
    let close = close_of(&lines, open)?;
    let inner = lines.get(open.saturating_add(1)..close)?;
    if inner.iter().any(|l| l.trim_start().starts_with("<FXCHAIN")) {
        return None;
    }
    let at = inner
        .iter()
        .position(|l| l.trim_start().starts_with("<ITEM"))
        .map_or(close, |i| open.saturating_add(1).saturating_add(i));
    let pad = " ".repeat(indent(lines.get(open)?).saturating_add(2));
    let mut out: Vec<String> = lines.get(..at)?.iter().map(|l| (*l).to_string()).collect();
    out.extend(block.lines().map(|l| format!("{pad}{l}")));
    out.extend(lines.get(at..)?.iter().map(|l| (*l).to_string()));
    Some(out.join("\n") + "\n")
}

/// The folder a song's own files go in: the one it was opened from, or
/// for a `.session` folder's song the folder that holds it. Its media
/// paths, relative to that folder, still find the audio.
///
/// # Errors
///
/// No such project, or it has no file it was opened from.
pub fn beside(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
) -> eyre::Result<std::path::PathBuf> {
    let from = daw
        .info(ProjectContext::Project(project_guid.to_owned()))?
        .path;
    let dir = std::path::Path::new(&from)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .ok_or_else(|| eyre::eyre!("the song has no file it was opened from: {from:?}"))?;
    // A `.session` folder's project is written beside the folder.
    let dir = if dir.extension().is_some_and(|e| e == "session") {
        dir.parent().unwrap_or(dir)
    } else {
        dir
    };
    Ok(dir.to_path_buf())
}

/// The name [`export`] gives a song's project when it is given none: the
/// song's own, marked as Studio's, so it never writes over the file the
/// song came from.
#[must_use]
pub fn default_name(song: &str) -> String {
    format!("{} (Studio)", song.trim())
}

/// Write `project_guid` as a REAPER project named `name` (`.RPP` added)
/// beside the file it was opened from ([`beside`]), the session's own
/// tracks given the template's chains ([`chain_for`]).
///
/// # Errors
///
/// No such project, no file it was opened from, a `name` that is a path
/// or would write over that file, or the write failed.
pub fn write_rpp(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    name: &str,
) -> eyre::Result<Exported> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        eyre::bail!("{name:?} is not a file name");
    }
    let project = ProjectContext::Project(project_guid.to_owned());
    let dir = beside(daw, project_guid)?;
    let path = dir.join(format!("{name}.RPP"));
    let from = daw.info(project.clone())?.path;
    if same_file(&path, std::path::Path::new(&from)) {
        eyre::bail!(
            "{} is the file the song was opened from: give another name",
            path.display()
        );
    }
    let mut text =
        daw_standalone::session_file::project_rpp_text_relative_to(daw, project_guid, Some(&dir))
            .map_err(|e| eyre::eyre!("project as RPP: {e}"))?;
    let mut chained = Vec::new();
    for track in Tracks::all(daw, project.clone()) {
        let Some(block) = chain_for(&track.name) else {
            continue;
        };
        if let Some(with) = splice(&text, &track.guid, &block) {
            text = with;
            chained.push(track.name.clone());
        }
    }
    std::fs::write(&path, text)?;
    Ok(Exported { path, chained })
}

/// Whether `a` and `b` name one file (case aside, as macOS's disks are).
fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

static EXPORT: architect::action::ActionMeta = crate::actions::meta(
    "MIX_EXPORT",
    "mix_export",
    "Export the session as a REAPER project",
    "Write the open song as a REAPER project (`.RPP`) beside the file it was opened from (a `.session` folder's song beside the folder), so its media paths still find the audio and REAPER opens it as it is: every track, item, fader, send and the FX chain each track came with; the tracks the session made itself (the buses, the drum Process folder) get the golden template's chains as REAPER's stock plugins. `name`: the file's name without `.RPP` (default: the song's, with ` (Studio)`); never the file the song was opened from. Touches neither playback nor the song. Says the `file`, how many `tracks` it holds and the tracks given a template chain (`chained`).",
    "Mix",
    "",
);

/// What to export.
#[derive(facet::Facet)]
struct ExportArg {
    name: Option<String>,
}

/// What [`export`] wrote, as MIX_EXPORT answers it.
#[derive(facet::Facet, Debug)]
pub struct ExportRow {
    pub file: String,
    pub tracks: u32,
    pub chained: Vec<String>,
}

/// `MIX_EXPORT`: `project_guid` as a REAPER project named `name` (or
/// [`default_name`]) beside its file.
///
/// # Errors
///
/// As [`write_rpp`].
pub fn export(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    name: Option<&str>,
) -> eyre::Result<ExportRow> {
    let project = ProjectContext::Project(project_guid.to_owned());
    let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => name.to_owned(),
        None => default_name(&daw.info(project.clone())?.name),
    };
    let done = write_rpp(daw, project_guid, &name)?;
    let tracks = u32::try_from(Tracks::all(daw, project).len()).unwrap_or(u32::MAX);
    Ok(ExportRow {
        file: done.path.display().to_string(),
        tracks,
        chained: done.chained,
    })
}

/// Register `MIX_EXPORT`.
///
/// # Errors
///
/// The list refuses it.
pub fn register(
    list: &architect::action::ActionList,
) -> Result<(), architect::action::ActionError> {
    list.register(
        &EXPORT,
        architect::action::Effect::ReadOnly,
        |a: ExportArg| {
            let _s = tracing::info_span!("MIX_EXPORT").entered();
            let (daw, project) = crate::actions::song()?;
            let guid = crate::moment::project_guid(&project)?;
            export(daw, &guid, a.name.as_deref()).map_err(|e| format!("{e:#}"))
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use daw::service::Tracks as _;

    use super::{default_name, export};

    /// The test song, opened from `Song.RPP` in a folder of its own.
    fn song_in(dir: &std::path::Path) -> (daw_standalone::sync::Standalone, String) {
        let from = dir.join("Song.RPP");
        std::fs::write(&from, "<REAPER_PROJECT 0.1 \"7.0\" 0\n>\n").expect("the song's file");
        let (daw, _) = crate::test_song::song_at("Song", None, &from.to_string_lossy());
        (daw, "Song".to_owned())
    }

    #[test]
    fn mix_export_writes_a_reaper_project_beside_the_song() {
        let dir = tempfile::tempdir().expect("a folder");
        let (daw, guid) = song_in(dir.path());
        let out = export(&daw, &guid, None).expect("exported");
        let file = dir.path().join(format!("{}.RPP", default_name("Song")));
        assert_eq!(out.file, file.display().to_string());
        let text = std::fs::read_to_string(&file).expect("the RPP");
        let project = dawfile_reaper::parse_project_text(&text).expect("REAPER reads it");
        let names: Vec<String> = project.tracks.iter().map(|t| t.name.clone()).collect();
        let ours: Vec<String> = daw
            .all(daw::service::ProjectContext::Project(guid))
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, ours);
        assert_eq!(out.tracks as usize, ours.len());
        // The song's own file is as it was.
        let original = std::fs::read_to_string(dir.path().join("Song.RPP")).expect("the song");
        assert_eq!(original, "<REAPER_PROJECT 0.1 \"7.0\" 0\n>\n");
    }

    #[test]
    fn mix_export_never_writes_over_the_song_or_out_of_its_folder() {
        let dir = tempfile::tempdir().expect("a folder");
        let (daw, guid) = song_in(dir.path());
        assert!(export(&daw, &guid, Some("Song")).is_err());
        assert!(export(&daw, &guid, Some("../elsewhere")).is_err());
        let named = export(&daw, &guid, Some("For Mixing")).expect("exported");
        assert!(named.file.ends_with("For Mixing.RPP"), "{}", named.file);
    }

    #[test]
    fn mix_export_refuses_a_song_with_no_file() {
        let (daw, _) = crate::test_song::song("nowhere", None);
        let err = export(&daw, "nowhere", None).expect_err("no folder to write in");
        assert!(err.to_string().contains("no file"), "{err}");
    }
}
