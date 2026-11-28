//! Studio's home and Signal's, on every machine (`apps/studio/docs/storage.md`).
//!
//! The **Studio home** is, in order: `STUDIO_HOME`; `home = "…"` in
//! `<config dir>/fasttrackstudio/studio.toml`; `~/Drive/Studio` when the
//! Drive mount is there; `<config dir>/fasttrackstudio/Studio`.
//!
//! The **Signal home** is `<Studio home>/Signal` (`SIGNAL_HOME` overrides
//! it): the rig's NAM captures, IRs, presets and sample libraries. With
//! Drive it is `~/Drive/Studio/Signal`, the same folder on every computer.
//!
//! The **Libraries home** is `<Studio home>/Libraries`: the raw sample
//! libraries (`Sampled/…`, `Kontakt/…`), far too big to keep on every
//! computer. With Drive it is `~/Drive/Studio/Libraries`, beside
//! `Studio/Signal` (Drive's adoptions never nest).
//!
//! A rig file is named by its path **in the Signal home**
//! (`NAM_models/Amp.nam`, `Libraries/Drum Kits/…`), never by a disk's path
//! ([`stored`]). Presets, songs and code written before that name it by the
//! disk it was on (`/run/media/AudioHaven/Signal/…`, [`LEGACY_ROOTS`]);
//! those paths are read as the same file in its home ([`locate`]), so they
//! open on a machine that has no such disk, without being rewritten.

use std::path::{Path, PathBuf};

/// The part of the Drive tree a rig file is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    /// The Signal home, `Studio/Signal`: NAM captures, IRs, presets, packs.
    Signal,
    /// The Libraries home, `Studio/Libraries`: the raw sample libraries.
    Libraries,
}

impl Area {
    /// Its place in the Drive tree.
    #[must_use]
    pub const fn in_drive(self) -> &'static str {
        match self {
            Self::Signal => SIGNAL_IN_DRIVE,
            Self::Libraries => LIBRARIES_IN_DRIVE,
        }
    }
}

/// The folders old presets, songs and code name rig files on, and where
/// each is now: `X` under `disk` is `<folder>/X` in `area`'s home.
///
/// - AudioHaven's `Signal` is the Signal home itself (adopted).
/// - The rig-models clone CI keeps on THEBATTLESHIP (Unbreakable's bass and
///   guitar presets name it): a copy is in `rig-library/<kind>/models`, laid
///   out as `assets.txt` links it into the tree.
/// - AudioHaven's `Sampled` and `Kontakt` libraries (adopted).
pub const LEGACY_ROOTS: &[(&str, Area, &str)] = &[
    ("/run/media/AudioHaven/Signal", Area::Signal, ""),
    (
        "/run/media/Development/ci-assets/rig-models/bass",
        Area::Signal,
        "rig-library/bass/models",
    ),
    (
        "/run/media/Development/ci-assets/rig-models/guitar",
        Area::Signal,
        "rig-library/guitar/models",
    ),
    ("/run/media/AudioHaven/Sampled", Area::Libraries, "Sampled"),
    ("/run/media/AudioHaven/Kontakt", Area::Libraries, "Kontakt"),
];

/// The Signal home's place in the Drive tree, for what a person is told to
/// pin.
pub const SIGNAL_IN_DRIVE: &str = "Studio/Signal";

/// The Libraries home's place in the Drive tree.
pub const LIBRARIES_IN_DRIVE: &str = "Studio/Libraries";

/// The config directory: `$XDG_CONFIG_HOME` or `~/.config` on Linux,
/// `~/Library/Application Support` on macOS and iOS, `%APPDATA%` on
/// Windows.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        env_path("APPDATA")
    } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        env_path("HOME").map(|h| h.join("Library").join("Application Support"))
    } else {
        env_path("XDG_CONFIG_HOME").or_else(|| env_path("HOME").map(|h| h.join(".config")))
    }
}

/// The Studio home, found now (see the module's doc). A caller that must
/// not see it move during a run (a Drive mount dropping out) keeps the
/// first answer.
#[must_use]
pub fn find_studio_home() -> Option<PathBuf> {
    if let Some(d) = env_path("STUDIO_HOME") {
        return Some(d);
    }
    let config = config_dir()?.join("fasttrackstudio");
    if let Some(h) = std::fs::read_to_string(config.join("studio.toml"))
        .ok()
        .and_then(|t| home_setting(&t))
    {
        return Some(expand_home(&h));
    }
    let mount = env_path("HOME").map(|h| h.join("Drive").join("Studio"));
    // A mount that does not answer is still Drive's home: it must not move
    // to the config folder because Drive is slow to answer.
    if let Some(m) = mount.filter(|m| probe(m, |p| p.is_dir()).unwrap_or(true)) {
        return Some(m);
    }
    Some(config.join("Studio"))
}

/// How long a look at a file in the Drive mount may take before the mount
/// is taken as not answering.
const PROBE_WAIT: std::time::Duration = std::time::Duration::from_secs(2);
/// How long a mount that did not answer is left alone before it is asked
/// again.
const PROBE_QUIET: std::time::Duration = std::time::Duration::from_secs(30);

/// `look` at `path`, given at most [`PROBE_WAIT`]: a FUSE mount (Drive's)
/// that hangs must never hang a loader. `None` when it did not answer, or
/// when one did not answer in the last [`PROBE_QUIET`]. Natively the look
/// runs on its own thread (left behind if the mount never answers); a
/// browser has no mount and no threads, and looks directly.
fn probe(path: &Path, look: fn(&Path) -> bool) -> Option<bool> {
    #[cfg(target_arch = "wasm32")]
    {
        Some(look(path))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::Mutex;
        static SILENT_SINCE: Mutex<Option<std::time::Instant>> = Mutex::new(None);
        let silent = |m: &Mutex<Option<std::time::Instant>>| {
            *m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        if silent(&SILENT_SINCE).is_some_and(|at| at.elapsed() < PROBE_QUIET) {
            return None;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let owned = path.to_path_buf();
        std::thread::Builder::new()
            .name("signal-home-probe".into())
            .spawn(move || {
                let _ = tx.send(look(&owned));
            })
            .ok()?;
        let answer = rx.recv_timeout(PROBE_WAIT).ok();
        if answer.is_none() {
            *SILENT_SINCE
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(std::time::Instant::now());
        }
        answer
    }
}

/// The Studio home, chosen once a run.
#[must_use]
pub fn studio_home() -> Option<PathBuf> {
    static HOME: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    HOME.get_or_init(find_studio_home).clone()
}

/// The Signal home: `SIGNAL_HOME`, else `<Studio home>/Signal`.
#[must_use]
pub fn signal_home() -> Option<PathBuf> {
    env_path("SIGNAL_HOME").or_else(|| studio_home().map(|h| h.join("Signal")))
}

/// The Libraries home: `<Studio home>/Libraries`.
#[must_use]
pub fn libraries_home() -> Option<PathBuf> {
    studio_home().map(|h| h.join("Libraries"))
}

/// The homes a rig file is looked for in, one per [`Area`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Homes<'a> {
    /// The Signal home.
    pub signal: Option<&'a Path>,
    /// The Libraries home.
    pub libraries: Option<&'a Path>,
}

impl<'a> Homes<'a> {
    fn of(self, area: Area) -> Option<&'a Path> {
        match area {
            Area::Signal => self.signal,
            Area::Libraries => self.libraries,
        }
    }
}

/// `home = "…"` in the settings file (a line of TOML; nothing else is read).
#[must_use]
pub fn home_setting(toml: &str) -> Option<String> {
    toml.lines()
        .find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == "home")
                .then(|| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        })
        .filter(|v| !v.is_empty())
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), env_path("HOME")) {
        (Some(rest), Some(h)) => h.join(rest),
        _ => PathBuf::from(p),
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The Signal home's top folders: a relative path starting with one is a
/// path in the Signal home.
const SIGNAL_FOLDERS: &[&str] = &[
    "IR",
    "Libraries",
    "NAM",
    "NAM_models",
    "Orchestral",
    "Patches",
    "Reference",
    "Songs",
    "rig-library",
];

/// A rig file's place: the [`Area`] it is in and its `/`-separated path in
/// that area's home (`NAM_models/Amp.nam`, `Sampled/Guitar/Shreddage Abyss`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    /// The home it is in.
    pub area: Area,
    /// Its path in that home.
    pub rel: String,
}

impl Place {
    /// Its path in the Drive tree (`Studio/Signal/NAM_models/Amp.nam`): what
    /// to pin.
    #[must_use]
    pub fn in_drive(&self) -> String {
        format!("{}/{}", self.area.in_drive(), self.rel)
    }
}

/// `path`'s place ([`Place`]): for a path under one of [`LEGACY_ROOTS`] or
/// under the Signal home `signal`, and for a relative path that starts at
/// one of the Signal home's folders. `None` for anything else.
#[must_use]
pub fn place(path: &str, signal: Option<&Path>) -> Option<Place> {
    let path = path.trim();
    let unixy = path.replace('\\', "/");
    let under = |root: &str| -> Option<String> {
        let root = root.trim_end_matches('/');
        unixy
            .strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
            .map(str::to_owned)
    };
    let (area, rel) = LEGACY_ROOTS
        .iter()
        .find_map(|&(disk, area, folder)| {
            let rest = under(disk)?;
            Some((area, normal(&rest).map(|r| joined(folder, &r))))
        })
        .or_else(|| {
            let home = signal?.to_string_lossy().replace('\\', "/");
            Some((Area::Signal, normal(&under(&home)?)))
        })
        .or_else(|| {
            let first = unixy.split('/').next()?;
            (!Path::new(path).is_absolute() && SIGNAL_FOLDERS.contains(&first))
                .then(|| (Area::Signal, normal(&unixy)))
        })?;
    Some(Place { area, rel: rel? })
}

/// `rel` with `.` and `..` taken out (a kit's
/// `Presets/../Engines/x.signalengine`); `None` when it climbs out of its
/// root or is empty.
fn normal(rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn joined(folder: &str, rel: &str) -> String {
    if folder.is_empty() {
        rel.to_owned()
    } else {
        format!("{folder}/{rel}")
    }
}

/// `path`'s place in the Signal home `home`, as `/`-separated text
/// (`NAM_models/Amp.nam`): a [`place`] in [`Area::Signal`].
#[must_use]
pub fn in_signal(path: &str, home: Option<&Path>) -> Option<String> {
    place(path, home)
        .filter(|p| p.area == Area::Signal)
        .map(|p| p.rel)
}

/// How `path` is written into a preset, a song or a config: its place in
/// the Signal home when it is in it (or on an old Signal disk), else as it
/// is.
#[must_use]
pub fn stored(path: &str) -> String {
    in_signal(path, signal_home().as_deref()).unwrap_or_else(|| path.to_owned())
}

/// A rig file Drive has not brought to this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotInDrive {
    /// The home it is in.
    pub area: Area,
    /// Its path in that home (`NAM_models/Amp.nam`).
    pub rel: String,
    /// Where it was looked for.
    pub looked: PathBuf,
    /// The Drive mount did not answer (not that the file is missing).
    pub silent: bool,
}

impl NotInDrive {
    /// Its path in the Drive tree (`Studio/Signal/NAM_models/Amp.nam`).
    #[must_use]
    pub fn in_drive(&self) -> String {
        format!("{}/{}", self.area.in_drive(), self.rel)
    }

    /// The `drive pin` command that brings it here, quoted for a shell.
    #[must_use]
    pub fn pin_command(&self) -> String {
        format!("drive pin \"{}\"", self.in_drive())
    }
}

impl std::fmt::Display for NotInDrive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let at = self.in_drive();
        let looked = self.looked.display();
        if self.silent {
            return write!(
                f,
                "{at} could not be read: the Drive mount ({looked}) did not answer. Check that \
                 Drive is running (`drive status \"{at}\"`) and open the song again",
            );
        }
        let pin = self.pin_command();
        match self.area {
            Area::Signal => write!(
                f,
                "{at} is not on this computer (looked at {looked}): it is not in Drive here yet. \
                 Pin it (`{pin}`, or a folder above it) and open the song again",
            ),
            // A library is hundreds of GB: pin the one a song plays, never
            // all of Studio/Libraries.
            Area::Libraries => write!(
                f,
                "{at} is not on this computer (looked at {looked}): this sample library is not \
                 in Drive here yet. Pin it for the songs that play it (`{pin}`; not all of \
                 {LIBRARIES_IN_DRIVE}, which is too big) and open the song again",
            ),
        }
    }
}

impl std::error::Error for NotInDrive {}

/// The file on this computer that the rig file `path` names, in `homes`:
///
/// - a path on an old disk ([`LEGACY_ROOTS`]) that this computer has is
///   itself: the same bytes, read without the Drive mount;
/// - any other path with a [`place`] (relative, or on an old disk this
///   computer lacks) is `<its area's home>/<rel>`;
/// - any other path is itself (a path already under a home is a path on
///   this computer).
///
/// The home is looked at with a time limit: a Drive mount that hangs gives
/// an error, never a hang.
///
/// # Errors
///
/// [`NotInDrive`] for a path in the Signal home whose file is not here, or
/// when the mount did not answer.
pub fn locate_in(path: &str, homes: Homes<'_>) -> Result<PathBuf, NotInDrive> {
    let given = PathBuf::from(path);
    let Some(Place { area, rel }) = place(path, None) else {
        return Ok(given);
    };
    if given.is_absolute() && given.exists() {
        return Ok(given);
    }
    let Some(at_home) = homes.of(area).map(|h| h.join(&rel)) else {
        return Err(NotInDrive {
            area,
            rel,
            looked: given,
            silent: false,
        });
    };
    match probe(&at_home, |p| p.exists()) {
        Some(true) => Ok(at_home),
        found => Err(NotInDrive {
            area,
            rel,
            looked: at_home,
            silent: found.is_none(),
        }),
    }
}

/// [`locate_in`] this machine's homes ([`signal_home`],
/// [`libraries_home`]), found only for a path that has a [`place`].
///
/// # Errors
///
/// [`NotInDrive`] for a path in the Signal home whose file is not here.
pub fn locate(path: &str) -> Result<PathBuf, NotInDrive> {
    let Some(found) = place(path, None) else {
        return Ok(PathBuf::from(path));
    };
    let home = match found.area {
        Area::Signal => signal_home(),
        Area::Libraries => libraries_home(),
    };
    let homes = Homes {
        signal: home.as_deref().filter(|_| found.area == Area::Signal),
        libraries: home.as_deref().filter(|_| found.area == Area::Libraries),
    };
    locate_in(path, homes)
}

/// [`locate`], for a loader that reports its own error: the file found, or
/// the path as given when there is none.
#[must_use]
pub fn located(path: &str) -> PathBuf {
    locate(path).unwrap_or_else(|_| PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "/run/media/AudioHaven/Signal/NAM_models/Helga B Boss OS-2.nam";

    fn signal(home: &Path) -> Homes<'_> {
        Homes {
            signal: Some(home),
            libraries: None,
        }
    }

    fn home_with(files: &[&str]) -> tempfile_lite::Dir {
        let dir = tempfile_lite::Dir::new();
        for f in files {
            let p = dir.path().join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        dir
    }

    #[test]
    fn an_old_audiohaven_path_is_its_place_in_the_signal_home() {
        assert_eq!(
            in_signal(OLD, None).as_deref(),
            Some("NAM_models/Helga B Boss OS-2.nam")
        );
        assert_eq!(
            in_signal(
                "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD/Presets/A.signalpreset",
                None
            )
            .as_deref(),
            Some("Libraries/Drum Kits/GGD/Presets/A.signalpreset")
        );
        assert_eq!(in_signal("/run/media/AudioHaven/Sampled/x.wav", None), None);
        assert_eq!(in_signal("/run/media/AudioHaven/Signal/../etc", None), None);
        assert_eq!(
            in_signal(
                "Libraries/Drum Kits/GGD/Presets/../Engines/Kick.signalengine",
                None
            )
            .as_deref(),
            Some("Libraries/Drum Kits/GGD/Engines/Kick.signalengine")
        );
        assert_eq!(in_signal("models/x.nam", None), None);
        assert_eq!(
            in_signal("IR/EchoThief/a.wav", None).as_deref(),
            Some("IR/EchoThief/a.wav")
        );
        assert_eq!(
            in_signal(
                "/home/me/Drive/Studio/Signal/IR/a.wav",
                Some(Path::new("/home/me/Drive/Studio/Signal"))
            )
            .as_deref(),
            Some("IR/a.wav")
        );
    }

    #[test]
    fn an_old_audiohaven_path_resolves_to_the_signal_home() {
        let home = home_with(&["NAM_models/Helga B Boss OS-2.nam"]);
        // The old disk itself where this computer has it (THEBATTLESHIP),
        // else the file in the Signal home.
        let want = if Path::new(OLD).exists() {
            PathBuf::from(OLD)
        } else {
            home.path().join("NAM_models/Helga B Boss OS-2.nam")
        };
        assert_eq!(locate_in(OLD, signal(home.path())).unwrap(), want);
        assert_eq!(
            locate_in("NAM_models/Helga B Boss OS-2.nam", signal(home.path())).unwrap(),
            home.path().join("NAM_models/Helga B Boss OS-2.nam")
        );
    }

    #[test]
    fn a_file_not_in_drive_yet_says_to_pin_it() {
        let home = home_with(&[]);
        let err = locate_in(
            "/run/media/AudioHaven/Signal/Libraries/Drum Kits/GGD/Presets/Gone.signalpreset",
            signal(home.path()),
        )
        .unwrap_err();
        assert_eq!(err.rel, "Libraries/Drum Kits/GGD/Presets/Gone.signalpreset");
        let said = err.to_string();
        assert!(said.contains("not in Drive here yet"), "{said}");
        assert!(
            said.contains(
                "drive pin \"Studio/Signal/Libraries/Drum Kits/GGD/Presets/Gone.signalpreset\""
            ),
            "{said}"
        );
    }

    #[test]
    fn a_path_outside_signal_is_left_as_it_is() {
        let home = home_with(&[]);
        assert_eq!(
            locate_in("/tmp/elsewhere/x.nam", signal(home.path())).unwrap(),
            PathBuf::from("/tmp/elsewhere/x.nam")
        );
    }

    #[test]
    fn a_rig_file_is_stored_by_its_place_in_the_signal_home() {
        let home = Path::new("/home/me/Drive/Studio/Signal");
        assert_eq!(
            in_signal(OLD, Some(home)).as_deref(),
            Some("NAM_models/Helga B Boss OS-2.nam")
        );
        assert_eq!(in_signal("/tmp/x.nam", Some(home)), None);
    }

    #[test]
    fn a_path_outside_signal_never_looks_for_the_home() {
        // Not even `signal_home()`: an unrelated load never waits on Drive.
        assert_eq!(locate("/tmp/x.nam").unwrap(), PathBuf::from("/tmp/x.nam"));
        assert_eq!(
            locate("models/x.nam").unwrap(),
            PathBuf::from("models/x.nam")
        );
    }

    #[test]
    fn ci_assets_rig_models_are_the_rig_library_in_the_signal_home() {
        let amp = "/run/media/Development/ci-assets/rig-models/guitar/1966 AC30 Bril OD.nam";
        assert_eq!(
            in_signal(amp, None).as_deref(),
            Some("rig-library/guitar/models/1966 AC30 Bril OD.nam")
        );
        assert_eq!(
            in_signal(
                "/run/media/Development/ci-assets/rig-models/bass/PARALLAX DEFAULT.nam",
                None
            )
            .as_deref(),
            Some("rig-library/bass/models/PARALLAX DEFAULT.nam")
        );
        // Only the two capture folders: the clone's README is not a rig file.
        assert_eq!(
            in_signal(
                "/run/media/Development/ci-assets/rig-models/README.md",
                None
            ),
            None
        );
        assert_eq!(
            in_signal(
                "/run/media/Development/ci-assets/rig-models/guitar/../../x",
                None
            ),
            None
        );
        let home = home_with(&["rig-library/guitar/models/1966 AC30 Bril OD.nam"]);
        let want = if Path::new(amp).exists() {
            PathBuf::from(amp) // THEBATTLESHIP: CI's own clone, read directly
        } else {
            home.path()
                .join("rig-library/guitar/models/1966 AC30 Bril OD.nam")
        };
        assert_eq!(locate_in(amp, signal(home.path())).unwrap(), want);
        let gone = "/run/media/Development/ci-assets/rig-models/bass/Gone.nam";
        let said = locate_in(gone, signal(home.path()))
            .unwrap_err()
            .to_string();
        assert!(
            said.contains("drive pin \"Studio/Signal/rig-library/bass/models/Gone.nam\""),
            "{said}"
        );
    }

    #[test]
    fn sampled_and_kontakt_libraries_are_in_the_libraries_home() {
        let abyss = "/run/media/AudioHaven/Sampled/Guitar/Shreddage Abyss";
        assert_eq!(
            place(abyss, None),
            Some(Place {
                area: Area::Libraries,
                rel: "Sampled/Guitar/Shreddage Abyss".into(),
            })
        );
        let kontakt = "/run/media/AudioHaven/Kontakt/Drums/Kit/a.nkx";
        let at = place(kontakt, None).unwrap();
        assert_eq!(at.area, Area::Libraries);
        assert_eq!(at.in_drive(), "Studio/Libraries/Kontakt/Drums/Kit/a.nkx");
        // Not the Signal home's: a Signal-only caller leaves them alone.
        assert_eq!(in_signal(abyss, None), None);

        let libraries = home_with(&["Sampled/Guitar/Shreddage Abyss/_patches/x.styx"]);
        let homes = Homes {
            signal: None,
            libraries: Some(libraries.path()),
        };
        let patch = format!("{abyss}/_patches/x.styx");
        let want = if Path::new(&patch).exists() {
            PathBuf::from(&patch)
        } else {
            libraries
                .path()
                .join("Sampled/Guitar/Shreddage Abyss/_patches/x.styx")
        };
        assert_eq!(locate_in(&patch, homes).unwrap(), want);
    }

    #[test]
    fn a_library_not_in_drive_yet_says_to_pin_that_library() {
        let libraries = home_with(&[]);
        let homes = Homes {
            signal: None,
            libraries: Some(libraries.path()),
        };
        let err =
            locate_in("/run/media/AudioHaven/Sampled/Guitar/Gone Library", homes).unwrap_err();
        assert_eq!(err.area, Area::Libraries);
        let said = err.to_string();
        assert!(
            said.contains("drive pin \"Studio/Libraries/Sampled/Guitar/Gone Library\""),
            "{said}"
        );
        assert!(said.contains("not all of Studio/Libraries"), "{said}");
        let err = locate_in("/run/media/AudioHaven/Kontakt/Gone/a.nki", homes).unwrap_err();
        assert_eq!(
            err.pin_command(),
            "drive pin \"Studio/Libraries/Kontakt/Gone/a.nki\""
        );
    }

    #[test]
    fn the_home_setting() {
        assert_eq!(
            home_setting("# Studio\nhome = \"/home/me/Drive/Studio\"\n").as_deref(),
            Some("/home/me/Drive/Studio")
        );
        assert_eq!(home_setting("theme = 'dark'\n"), None);
        assert_eq!(home_setting("home = ''"), None);
    }

    /// A temporary folder, removed when dropped (no dev-dependency).
    mod tempfile_lite {
        pub struct Dir(std::path::PathBuf);
        impl Dir {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicU32, Ordering};
                static N: AtomicU32 = AtomicU32::new(0);
                let p = std::env::temp_dir().join(format!(
                    "utils-home-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&p).unwrap();
                Self(p)
            }
            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}
