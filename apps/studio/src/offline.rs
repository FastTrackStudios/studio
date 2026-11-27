//! A setlist downloaded for offline (studio#716): the "Download for
//! offline" button on a setlist, and what it keeps on this device.
//!
//! - **Desktop and phone:** the engine downloads the set
//!   (`SETLIST_DOWNLOAD`, `crate::engine::download_setlist`) from each
//!   song's session folder on this computer into `<home>/Offline/<set>/`:
//!   the full files on a desktop, the proxies on a phone. A setlist file
//!   beside the set's others, `<home>/Setlists/<set> (offline).setlist`,
//!   names those copies, so it is in the setlists and opens from disk with
//!   no internet.
//! - **Web:** each song's session folder is brought into the browser
//!   (`crate::engine_web::keep_offline`): its listing and small files kept
//!   in the page's caches and every proxy fetched whole. The page itself
//!   and what it read through the node (the setlist file) are kept by its
//!   service worker (`public/sw.js`), so the set opens and plays with no
//!   network.

use dioxus::prelude::*;

use crate::data::Song;
use crate::icons::{I, Icon};
use crate::state::Place;

/// How a download stands.
#[derive(Clone, Debug, Default, PartialEq)]
enum Progress {
    #[default]
    Idle,
    /// Song `song` of `of` coming down, with its bytes so far of its total.
    Running {
        song: usize,
        of: usize,
        title: String,
        bytes: u64,
        total: u64,
    },
    /// Downloaded: what was kept, said.
    Done(String),
    Failed(String),
}

impl Progress {
    fn line(&self) -> Option<String> {
        match self {
            Self::Idle => None,
            Self::Running {
                song,
                of,
                title,
                bytes,
                total,
            } => Some(if *total > 0 {
                format!(
                    "Downloading {song} of {of}: {title} · {} of {}",
                    size(*bytes),
                    size(*total)
                )
            } else {
                format!("Downloading {song} of {of}: {title}")
            }),
            Self::Done(said) => Some(said.clone()),
            Self::Failed(why) => Some(format!("Not downloaded: {why}")),
        }
    }
}

fn size(n: u64) -> String {
    crate::files::bytes(i64::try_from(n).unwrap_or(i64::MAX))
}

/// "Download for offline", for a set's songs and where each one's session
/// folder is.
#[component]
pub fn OfflineButton(name: String, list: Vec<(Song, Place)>) -> Element {
    let mut progress = use_signal(|| Progress::Idle);
    let running = matches!(progress(), Progress::Running { .. });
    let kept_already = already_kept(&list);
    let go = move |_| {
        if running {
            return;
        }
        let (name, list) = (name.clone(), list.clone());
        progress.set(Progress::Running {
            song: 1,
            of: list.len(),
            title: list
                .first()
                .map(|(s, _)| s.title.to_string())
                .unwrap_or_default(),
            bytes: 0,
            total: 0,
        });
        spawn(async move {
            let outcome = download(&name, &list, progress).await;
            progress.set(match outcome {
                Ok(said) => Progress::Done(said),
                Err(why) => Progress::Failed(why),
            });
        });
    };
    let label = if running {
        "Downloading…"
    } else if kept_already && progress() == Progress::Idle {
        "Downloaded for offline"
    } else {
        "Download for offline"
    };
    rsx! {
        button {
            class: "playset quiet",
            disabled: running,
            title: "Keep this set on this device, to open and play with no internet",
            onclick: go,
            Icon { name: if kept_already { I::Check } else { I::Download }, size: 14 }
            "{label}"
        }
        if let Some(line) = progress().line() {
            p { class: "pline offline", "{line}" }
        }
    }
}

/// Whether every song of the set with a session folder is kept on this
/// device already.
fn already_kept(list: &[(Song, Place)]) -> bool {
    #[cfg(feature = "web")]
    {
        let mut any = false;
        for (_, place) in list {
            if place.folder.is_empty() {
                continue;
            }
            any = true;
            if !crate::engine_web::is_kept(place) {
                return false;
            }
        }
        any
    }
    #[cfg(not(feature = "web"))]
    {
        let _ = list;
        false
    }
}

/// In the browser: each song's session folder brought into the page's
/// caches, one after the other.
#[cfg(feature = "web")]
async fn download(
    _name: &str,
    list: &[(Song, Place)],
    mut progress: Signal<Progress>,
) -> Result<String, String> {
    let songs: Vec<&(Song, Place)> = list.iter().filter(|(_, p)| !p.folder.is_empty()).collect();
    if songs.is_empty() {
        return Err("none of its songs has a session folder".to_owned());
    }
    let of = songs.len();
    let mut bytes = 0u64;
    let mut failed = Vec::new();
    for (i, (song, place)) in songs.iter().enumerate() {
        progress.set(Progress::Running {
            song: i + 1,
            of,
            title: song.title.to_string(),
            bytes,
            total: 0,
        });
        match crate::engine_web::keep_offline(place).await {
            Ok(n) => bytes += n,
            Err(e) => {
                tracing::warn!(song.title = %song.title, error = %e, "offline: a song was not kept");
                failed.push(format!("{} ({e})", song.title));
            }
        }
    }
    if failed.len() == of {
        return Err(failed.join("; "));
    }
    let mut said = format!(
        "Downloaded {} of {of} songs ({}): this set opens and plays here with no internet",
        of - failed.len(),
        size(bytes)
    );
    if !failed.is_empty() {
        said.push_str(&format!(" · not kept: {}", failed.join("; ")));
    }
    Ok(said)
}

/// On a desktop or a phone: the engine downloads the set from each song's
/// session folder on this computer, off the UI thread, and the setlists
/// gain one naming the copies.
#[cfg(all(feature = "native", not(feature = "web")))]
async fn download(
    name: &str,
    list: &[(Song, Place)],
    mut progress: Signal<Progress>,
) -> Result<String, String> {
    let mut songs = Vec::new();
    let mut elsewhere = Vec::new();
    for (song, place) in list {
        if place.folder.is_empty() {
            continue;
        }
        let file = (place.location == crate::files::DISK)
            .then(|| crate::session_file::song_file(&place.folder))
            .flatten();
        match file {
            Some(file) => songs.push((song.title.to_string(), file)),
            None => elsewhere.push(song.title.to_string()),
        }
    }
    if songs.is_empty() {
        return Err(if elsewhere.is_empty() {
            "none of its songs has a session folder".to_owned()
        } else {
            format!(
                "its songs are on another computer ({}): open the set there, or bring the songs here",
                elsewhere.join(", ")
            )
        });
    }
    let home = crate::storage::home().ok_or("Studio has no home folder on this device")?;
    let folder_name = file_name(name);
    let to = home.join("Offline").join(&folder_name);
    let (tx, rx) = std::sync::mpsc::channel::<Result<serde_json::Value, String>>();
    let (set_name, wanted) = (name.to_owned(), songs.clone());
    std::thread::Builder::new()
        .name("studio-offline".into())
        .spawn(move || {
            let step = tx.clone();
            let done = crate::engine::download_setlist(&set_name, &wanted, &to, &|state| {
                let _ = step.send(Ok(state.clone()));
            });
            let _ = tx.send(done.map(|mut state| {
                state["done"] = serde_json::Value::Bool(true);
                state
            }));
        })
        .map_err(|e| format!("no thread to download on: {e}"))?;
    let finished = loop {
        crate::state::sleep_ms(250).await;
        let mut last = None;
        loop {
            match rx.try_recv() {
                Ok(message) => last = Some(message),
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if last.is_none() {
                        last = Some(Err("the download stopped".to_owned()));
                    }
                    break;
                }
            }
        }
        match last {
            Some(Ok(state)) if state["done"] == serde_json::Value::Bool(true) => break Ok(state),
            Some(Ok(state)) => progress.set(running(&state)),
            Some(Err(e)) => break Err(e),
            None => {}
        }
    }?;
    let lines = setlist_lines(&finished, &folder_name)?;
    let file = crate::storage::setlists()
        .ok_or("Studio has no setlists folder on this device")?
        .join(format!("{} (offline).setlist", file_name(name)));
    write_setlist(&file, name, &lines)?;
    let songs_down = finished["downloaded"].as_array().map_or(0, Vec::len);
    let bytes: u64 = finished["downloaded"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s["bytes"].as_u64())
        .sum();
    let mut said = format!(
        "Downloaded {songs_down} songs ({}, {}): \"{} (offline)\" in the setlists opens and plays here with no internet",
        size(bytes),
        finished["keep"].as_str().unwrap_or("full"),
        name
    );
    if !elsewhere.is_empty() {
        said.push_str(&format!(
            " · on another computer, not downloaded: {}",
            elsewhere.join(", ")
        ));
    }
    Ok(said)
}

/// `SETLIST_DOWNLOADING`, as a step of the download.
#[cfg(all(feature = "native", not(feature = "web")))]
fn running(state: &serde_json::Value) -> Progress {
    let count = |k: &str| usize::try_from(state[k].as_u64().unwrap_or(0)).unwrap_or(0);
    Progress::Running {
        song: count("song").max(1),
        of: count("songs").max(1),
        title: state["title"].as_str().unwrap_or_default().to_owned(),
        bytes: state["bytes"].as_u64().unwrap_or(0),
        total: state["total"].as_u64().unwrap_or(0),
    }
}

/// The lines of the setlist that names the downloaded copies, each a path
/// relative to the setlists folder (`../Offline/<set>/<song>`): the
/// engine's own `.setlist` lines (relative to the set's folder), moved
/// there.
#[cfg(all(feature = "native", not(feature = "web")))]
fn setlist_lines(finished: &serde_json::Value, folder_name: &str) -> Result<Vec<String>, String> {
    let written = finished["setlist"]
        .as_str()
        .ok_or("the engine wrote no setlist")?;
    let text = std::fs::read_to_string(written).map_err(|e| format!("{written}: {e}"))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| format!("../Offline/{folder_name}/{l}"))
        .collect())
}

#[cfg(all(feature = "native", not(feature = "web")))]
fn write_setlist(file: &std::path::Path, name: &str, lines: &[String]) -> Result<(), String> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut text = format!("# {name}, downloaded to play with no internet.\n");
    for line in lines {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(file, text).map_err(|e| format!("{}: {e}", file.display()))
}

/// A name for a folder or file: letters, digits, spaces and `-`.
#[cfg_attr(not(all(feature = "native", not(feature = "web"))), allow(dead_code))]
fn file_name(name: &str) -> String {
    let safe: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim().to_owned();
    if safe.is_empty() {
        "Setlist".to_owned()
    } else {
        safe
    }
}

/// Neither a browser nor the desktop engine: nothing to download with.
#[cfg(not(any(feature = "web", feature = "native")))]
async fn download(
    _name: &str,
    _list: &[(Song, Place)],
    _progress: Signal<Progress>,
) -> Result<String, String> {
    Err("this build has no engine to download a set with".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_name_is_a_folder_name_on_any_device() {
        assert_eq!(file_name("JT's Senior Recital"), "JT_s Senior Recital");
        assert_eq!(file_name("  "), "Setlist");
        assert_eq!(file_name("Merry & Bright 2026"), "Merry _ Bright 2026");
    }

    #[test]
    fn progress_says_how_far_it_is() {
        let p = Progress::Running {
            song: 2,
            of: 5,
            title: "Silent Night".into(),
            bytes: 0,
            total: 0,
        };
        assert_eq!(
            p.line().as_deref(),
            Some("Downloading 2 of 5: Silent Night")
        );
        assert_eq!(Progress::Idle.line(), None);
        assert!(
            Progress::Failed("x".into())
                .line()
                .unwrap()
                .starts_with("Not downloaded")
        );
    }
}
