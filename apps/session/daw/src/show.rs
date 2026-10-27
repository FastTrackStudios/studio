//! A show recorded as one Live set, made a setlist of songs.
//!
//! A recital or a gig is often one Live set: every song end to end on one
//! arrangement, marked the way AbleSet reads it (`ableton::setlist`). [`split`]
//! turns it into songs Session opens one by one:
//!
//! 1. the set is converted to an `.RPP` beside it and opened (media left
//!    unloaded), then saved as the show's own project file,
//!    `<Show>.session` — the whole show as one model;
//! 2. each song is that model cropped to the song's range and moved to start
//!    at zero (`session::sync::crop`), written fresh into
//!    `<into>/<slug>/<Song>.session`;
//! 3. the show's media folders (`Samples/` …) are linked into each song's
//!    folder, so a song names the very files the show does: nothing is
//!    copied, and the songs of a show share its recordings.
//!
//! What becomes of the songs — projects, a setlist in order — is the
//! caller's (`seed.sh jt`, the studio CLI).

use std::path::{Path, PathBuf};

use session::sync::{ORIGIN_LOCAL, SessionDoc};
use session_format::store::{MAIN, Store};

/// One song made from a show.
#[derive(Debug, Clone)]
pub struct Song {
    pub name: String,
    pub slug: String,
    /// Its project file.
    pub file: PathBuf,
    /// Where it sat in the show, seconds.
    pub start: f64,
    pub end: f64,
    /// Items it holds.
    pub items: usize,
}

/// Split the Live set at `set` into songs under `into` (one folder each,
/// named by slug). Returns them in show order.
///
/// # Errors
///
/// The set would not read, convert or open, or a song would not be written.
pub fn split(set: &Path, into: &Path) -> eyre::Result<Vec<Song>> {
    let live =
        ableton::io::read_live_set(set).map_err(|e| eyre::eyre!("{}: {e}", set.display()))?;
    let ranges = ableton::setlist::songs(&live);
    if ranges.is_empty() {
        eyre::bail!(
            "{}: no songs — mark each with a locator, and its end with SONG END",
            set.display()
        );
    }
    let show_dir = set.parent().unwrap_or(Path::new(".")).to_path_buf();
    let stem = set
        .file_stem()
        .map_or_else(|| "Show".to_owned(), |s| s.to_string_lossy().into_owned());

    // 1. The whole show as one model.
    let rpp = show_dir.join(format!("{stem}.RPP"));
    studio::file::convert(set, &rpp)
        .map_err(|e| eyre::eyre!("converting {}: {e}", set.display()))?;
    let model = show_model(&rpp, &show_dir.join(format!("{stem}.session")))?;
    // A set made on another machine names its media where it was there
    // (`/Users/jt/Music/Ableton/…/Samples/…`): re-rooted under the show.
    let model = relocate(model, &show_dir);

    // 2–3. Each song, cropped, beside links to the show's media.
    std::fs::create_dir_all(into)?;
    let mut songs = Vec::with_capacity(ranges.len());
    let titles: Vec<String> = ranges.iter().map(|r| r.name.clone()).collect();
    for range in ranges {
        let mut cropped = session::sync::crop::crop(&model, range.start_seconds, range.end_seconds);
        name_song(&mut cropped, &range.name, &titles);
        let dir = into.join(&range.slug);
        std::fs::create_dir_all(&dir)?;
        bring_media(&cropped, &show_dir, &dir)?;
        let file = dir.join(format!("{}.session", file_name(&range.name)));
        write_fresh(&file, &cropped)?;
        tracing::info!(
            show.song = %range.name,
            show.items = cropped.items.len(),
            show.seconds = range.seconds(),
            "show: song made"
        );
        songs.push(Song {
            name: range.name,
            slug: range.slug,
            file,
            start: range.start_seconds,
            end: range.end_seconds,
            items: cropped.items.len(),
        });
    }
    Ok(songs)
}

/// The show at `rpp` (media unloaded), saved as the project file `session`
/// and read back as one model.
fn show_model(rpp: &Path, session: &Path) -> eyre::Result<session::sync::SessionModel> {
    // Imported fresh every time: saving into an earlier import's file would
    // keep what that import made beside what this one does.
    for stale in [
        session.to_path_buf(),
        session.with_extension("session-wal"),
        session.with_extension("session-shm"),
        session.with_extension("session-tshm"),
    ] {
        if stale.exists() {
            std::fs::remove_file(&stale)?;
        }
    }
    let daw = daw::standalone::Standalone::new();
    let prepare = crate::prepare::Prepare::for_song(rpp);
    let (opened, _) =
        crate::open::open_song_into_with(&daw, rpp, &prepare, crate::open::Media::Deferred)?;
    crate::project_file::save(
        &daw,
        &opened.project_guid,
        session,
        MAIN,
        "Imported from Live",
    )
    .map_err(|e| eyre::eyre!("saving {}: {e}", session.display()))?;
    let store = Store::open(session).map_err(|e| eyre::eyre!("{}: {e}", session.display()))?;
    let doc = store
        .load(MAIN)
        .map_err(|e| eyre::eyre!("{}: {e}", session.display()))?;
    Ok(SessionDoc::from_loro(doc).read())
}

/// The song's own region (on the SONG lane — what its tab and the setlist
/// call it) named `name`: cut from the show, it still carried the show's
/// (`Sr. Recital Showcase - Session` on every tab). And no section named
/// for a song of the show (`titles`): the show's song locators became
/// sections when it was prepared, so `Cinematic Intro` ran on over Drive
/// To LA.
fn name_song(song: &mut session::sync::SessionModel, name: &str, titles: &[String]) {
    let lane = session::ruler_lanes::CoreLane::Song.lane_index();
    song.regions.retain(|_, region| {
        region.lane == Some(lane)
            || !titles
                .iter()
                .any(|t| t.trim().eq_ignore_ascii_case(region.name.trim()))
    });
    for region in song.regions.values_mut() {
        if region.lane == Some(lane) {
            name.clone_into(&mut region.name);
        }
    }
}

/// `model` with every take's media re-rooted under `show_dir` when it is
/// not where the set says: the shortest tail of its path that is there
/// (`Samples/Imported/Drums.wav`) — a set carried from the machine it was
/// made on keeps its media inside its own project folder.
fn relocate(
    mut model: session::sync::SessionModel,
    show_dir: &Path,
) -> session::sync::SessionModel {
    let mut moved = 0_usize;
    for item in model.items.values_mut() {
        for take in std::iter::once(&mut item.take).chain(item.other_takes.iter_mut()) {
            let Some(source) = take.source.as_deref() else {
                continue;
            };
            if let Some(rel) = rooted_under(Path::new(source), show_dir) {
                take.source = Some(rel);
                moved += 1;
            }
        }
    }
    tracing::info!(
        show.relocated = moved,
        "show: media re-rooted under the show"
    );
    model
}

/// `path` as the shortest tail that exists under `dir`, relative; `None`
/// when it already resolves where it says, or no tail of it does.
fn rooted_under(path: &Path, dir: &Path) -> Option<String> {
    if path.is_relative() && dir.join(path).exists() {
        return None;
    }
    if path.is_absolute() && path.exists() {
        return None;
    }
    let parts: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    (1..parts.len()).rev().find_map(|from| {
        let tail: PathBuf = parts[parts.len() - from..].iter().collect();
        dir.join(&tail)
            .exists()
            .then(|| tail.to_string_lossy().into_owned())
    })
}

/// The files `song` plays, put in its folder `dir` at the paths it names
/// them by (relative to the show): hard links to the show's own files, so a
/// song's folder holds real files a library serves and a device copies —
/// and costs no space, sharing the show's bytes on disk. Each file's proxy
/// (`Proxies/<stem>.ogg` and its page index) comes too when the show has
/// one. A link that cannot be made (another filesystem) is a copy.
fn bring_media(
    song: &session::sync::SessionModel,
    show_dir: &Path,
    dir: &Path,
) -> eyre::Result<()> {
    let mut files: Vec<PathBuf> = song
        .items
        .values()
        .flat_map(|i| i.takes())
        .filter_map(|t| t.source.as_deref())
        .map(PathBuf::from)
        .filter(|p| p.is_relative())
        .collect();
    files.sort();
    files.dedup();
    for rel in files {
        let from = show_dir.join(&rel);
        if !from.is_file() {
            continue;
        }
        link_file(&from, &dir.join(&rel))?;
        if let (Some(parent), Some(stem)) = (rel.parent(), rel.file_stem()) {
            let proxy = parent
                .join("Proxies")
                .join(format!("{}.ogg", stem.to_string_lossy()));
            for each in [
                proxy.clone(),
                PathBuf::from(format!("{}.idx", proxy.display())),
            ] {
                let from = show_dir.join(&each);
                if from.is_file() {
                    link_file(&from, &dir.join(&each))?;
                }
            }
        }
    }
    Ok(())
}

/// `to` as a hard link to `from` (made, or left when it already is one).
fn link_file(from: &Path, to: &Path) -> eyre::Result<()> {
    if to.symlink_metadata().is_ok() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::hard_link(from, to).is_err() {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

/// `model` written as a new project file at `file` (its song branch a fresh
/// doc of exactly this): a song starts small, with no history but its own.
fn write_fresh(file: &Path, model: &session::sync::SessionModel) -> eyre::Result<()> {
    if file.exists() {
        std::fs::remove_file(file)?;
    }
    let doc = SessionDoc::new();
    doc.write(model, ORIGIN_LOCAL)
        .map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
    let store = Store::open(file).map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
    store
        .rebase(MAIN, doc.loro())
        .map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
    // One file, its log folded in: what a library serves and a device
    // copies is the song whole, not half of it in a sidecar.
    store
        .checkpoint()
        .map_err(|e| eyre::eyre!("{}: {e}", file.display()))?;
    Ok(())
}

/// A song name as a file name: path separators and the like taken out.
fn file_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{file_name, name_song, rooted_under};

    #[test]
    fn media_from_the_machine_a_set_was_made_on_is_found_under_the_show() {
        let show = tempfile::tempdir().expect("a show folder");
        std::fs::create_dir_all(show.path().join("Samples/Imported")).expect("dirs");
        std::fs::write(show.path().join("Samples/Imported/Drums.wav"), b"").expect("a file");
        let elsewhere =
            std::path::Path::new("/Users/jt/Music/Ableton/Show Project/Samples/Imported/Drums.wav");
        assert_eq!(
            rooted_under(elsewhere, show.path()).as_deref(),
            Some("Samples/Imported/Drums.wav")
        );
        assert_eq!(
            rooted_under(
                std::path::Path::new("Samples/Imported/Drums.wav"),
                show.path()
            ),
            None,
            "already where it says"
        );
        assert_eq!(
            rooted_under(std::path::Path::new("/nowhere/Gone.wav"), show.path()),
            None
        );
    }

    #[test]
    fn a_song_cut_from_a_show_is_named_for_itself_with_no_other_song_as_a_section() {
        use session::sync::{RegionState, SessionModel};
        let region = |name: &str, lane: u32| RegionState {
            start: 0.0,
            end: 196.5,
            name: name.to_owned(),
            color: None,
            lane: Some(lane),
        };
        let mut song = SessionModel::default();
        song.regions
            .insert("song".into(), region("Sr. Recital Showcase - Session", 0));
        song.regions
            .insert("ran-on".into(), region("Cinematic Intro", 1));
        song.regions.insert("verse".into(), region("Verse 1", 1));
        let titles = ["Cinematic Intro".to_owned(), "Drive To LA".to_owned()];
        name_song(&mut song, "Drive To LA", &titles);
        let mut names: Vec<_> = song
            .regions
            .values()
            .map(|r| (r.lane, r.name.as_str()))
            .collect();
        names.sort_unstable();
        assert_eq!(names, vec![(Some(0), "Drive To LA"), (Some(1), "Verse 1")]);
    }

    #[test]
    fn a_song_name_makes_a_safe_file_name() {
        assert_eq!(file_name("Heart isn't Ready"), "Heart isn't Ready");
        assert_eq!(file_name("AC/DC: Live?"), "AC-DC- Live-");
    }
}
