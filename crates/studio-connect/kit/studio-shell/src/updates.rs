//! Desktop Studio updating itself from the studio repo's GitHub Releases.
//!
//! A release is a `studio-vYY.M.MICRO` tag (a pre-release while Studio is
//! in alpha) whose Release carries `Studio-<version>-macos-arm64.dmg`,
//! `Studio-<version>-linux-x86_64.AppImage`, a `SHA256SUMS` over them and
//! the sums' minisign signature by the org release key, with the trusted
//! comment `studio <tag> SHA256SUMS` (`.github/workflows/desktop.yml`,
//! xtask `sums.rs`). An update installs only when:
//!
//! 1. `SHA256SUMS.minisig` verifies against the key compiled into this
//!    build ([`RELEASE_KEY`]) and its trusted comment names this tag, so a
//!    signature lifted from an older release is refused;
//! 2. the download hashes to what the signed sums list for it;
//! 3. on macOS, the `.app` in the `.dmg` passes `codesign --verify
//!    --strict`, is signed by the same team as the running bundle, and its
//!    program answers `--version` with `studio <version>`.
//!
//! Download and verification are `architect::release`'s
//! ([`Updater::fetch_verified`]); what this module adds is the Studio
//! release feed (its tag prefix, its versioned asset names, pre-releases)
//! and putting the verified file in place of what was installed:
//!
//! - **macOS**: the `Studio.app` bundle (wherever it is, usually
//!   `/Applications`) is swapped for the `.dmg`'s copy, in one rename on
//!   the bundle's own volume ([`swap_bundle`]).
//! - **Linux**: the `AppImage` the process runs from (`$APPIMAGE`). One
//!   named `Studio-<version>-linux-x86_64.AppImage` (as `scripts/install.sh`
//!   names it) gets the new version's name beside it, and the `studio`
//!   link and desktop entry that pointed at the old one move to it
//!   ([`relink`]); any other name is replaced in place.
//!
//! The running process keeps its old files open; the new version runs from
//! the next launch. Every failure before the final rename leaves the
//! installed copy as it was.
//!
//! The deciding pieces are honest ([`pick`], [`Installation::detect`],
//! [`install_block`], [`run`], [`relink`], [`swap_bundle`]): feed, paths,
//! key and environment come in as parameters, so the tests drive them
//! against a fake feed. [`App`] is the skin the app calls, reading the
//! running binary, the compiled-in key and the environment.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use architect::release::{
    self, Candidate, GithubRelease, ReleaseError, SIG_ASSET, SUMS_ASSET, Updater,
};

/// Where Studio is released.
pub const REPO: &str = "FastTrackStudios/studio";

/// Studio's release tags: `studio-vYY.M.MICRO`.
pub const TAG_PREFIX: &str = "studio-v";

/// The name the release's signature carries (its trusted comment is
/// `studio <tag> SHA256SUMS`), and what `--version` prints before the
/// version.
pub const SIGNER: &str = "studio";

/// The org release key's public half: the base64 line of its minisign
/// `.pub`. Set at build time with `FTS_RELEASE_PUBKEY=RW…`; otherwise
/// [`ORG_RELEASE_KEY`].
pub const RELEASE_KEY: Option<&str> = match option_env!("FTS_RELEASE_PUBKEY") {
    Some(key) => Some(key),
    None => Some(ORG_RELEASE_KEY),
};

/// The org key every release's `SHA256SUMS` is signed with.
///
/// The same key the release workflow verifies with (xtask `sums.rs`, `ORG_RELEASE_KEY`)
/// and architect pins (`architect::release::ORG_KEY`). Its secret half is
/// the `FTS_RELEASE_MINISIGN_KEY` secret of the tag-only `release-signing`
/// environment.
pub const ORG_RELEASE_KEY: &str = "RWRh9Gk9qaStQMhku4THMidEYrQsmAEsm2dDprUkZxShEtqfIcroM8Od";

/// The bundle's name on macOS, in the `.dmg` and where it is installed.
pub const BUNDLE: &str = "Studio.app";

/// The program inside [`BUNDLE`] (`Contents/MacOS/Studio`).
const BUNDLE_PROGRAM: &str = "Contents/MacOS/Studio";

/// How long a check waits for the release list.
const LIST_TIMEOUT: Duration = Duration::from_secs(30);

/// The largest release list read (16 MiB).
const MAX_LIST_BYTES: u64 = 16 << 20;

// ── Platforms and what is installed ─────────────────────────────────

/// A platform Studio releases a desktop build for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    /// macOS on Apple silicon: the notarized `.dmg`.
    MacosArm64,
    /// Linux on `x86_64`: the `AppImage`.
    LinuxX86_64,
}

impl Platform {
    /// This build's platform, if Studio releases one for it.
    #[must_use]
    pub const fn current() -> Option<Self> {
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Self::MacosArm64)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Self::LinuxX86_64)
        } else {
            None
        }
    }

    /// The release asset for `version` (`Studio-26.11.2-macos-arm64.dmg`).
    #[must_use]
    pub fn asset(self, version: &str) -> String {
        match self {
            Self::MacosArm64 => format!("Studio-{version}-macos-arm64.dmg"),
            Self::LinuxX86_64 => format!("Studio-{version}-linux-x86_64.AppImage"),
        }
    }
}

/// How the running Studio was installed: what an update replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installation {
    /// A macOS bundle (the `.app` directory).
    MacApp(PathBuf),
    /// An `AppImage` file (`$APPIMAGE`).
    AppImage(PathBuf),
    /// A bare program (a build from source, `dx serve`, a package
    /// manager's copy): Studio checks but never replaces it.
    Unmanaged(PathBuf),
}

impl Installation {
    /// What `exe` (the running program, symlinks resolved) belongs to;
    /// `appimage` is `$APPIMAGE` (set by the `AppImage` runtime).
    #[must_use]
    pub fn detect(exe: &Path, appimage: Option<&Path>) -> Self {
        if let Some(image) = appimage.filter(|p| !p.as_os_str().is_empty()) {
            return Self::AppImage(image.to_owned());
        }
        exe.ancestors()
            .find(|p| p.extension().is_some_and(|ext| ext == "app"))
            .map_or_else(
                || Self::Unmanaged(exe.to_owned()),
                |b| Self::MacApp(b.to_owned()),
            )
    }

    /// The file or bundle an update replaces.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::MacApp(p) | Self::AppImage(p) | Self::Unmanaged(p) => p,
        }
    }

    /// `mac-app`, `appimage` or `unmanaged`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::MacApp(_) => "mac-app",
            Self::AppImage(_) => "appimage",
            Self::Unmanaged(_) => "unmanaged",
        }
    }
}

/// Why this Studio can check for updates but not install one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    /// The build pins no release key.
    NoKey,
    /// Studio releases no desktop build for this platform.
    NoPlatform,
    /// Not a release build (no `FTS_RELEASE_VERSION` when it was built): a
    /// development build is never replaced by a release.
    NotRelease,
    /// Not installed from a release (no `.app`, no `AppImage`).
    Unmanaged,
    /// The installation does not match this platform's release (an
    /// `AppImage` on macOS, a bundle on Linux).
    WrongKind,
    /// macOS runs the bundle from a randomized read-only copy (App
    /// Translocation), because it was opened where it was downloaded.
    Translocated,
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoKey => "This build pins no release key, so it checks but does not install.",
            Self::NoPlatform => "Studio has no release for this platform.",
            Self::NotRelease => "This is a development build: it checks but does not update itself.",
            Self::Unmanaged => {
                "This Studio was not installed from a release (.dmg or AppImage), so it checks but does not update itself."
            }
            Self::WrongKind => "This installation is not this platform's release.",
            Self::Translocated => "Move Studio to Applications to let it update itself.",
        })
    }
}

/// Why `installation` cannot take an update, if it cannot. `released`:
/// this binary is a release build.
#[must_use]
pub fn install_block(
    installation: &Installation,
    platform: Option<Platform>,
    key: Option<&str>,
    released: bool,
) -> Option<Block> {
    let Some(platform) = platform else {
        return Some(Block::NoPlatform);
    };
    if key.is_none() {
        return Some(Block::NoKey);
    }
    if !released {
        return Some(Block::NotRelease);
    }
    match (installation, platform) {
        (Installation::Unmanaged(_), _) => Some(Block::Unmanaged),
        (Installation::MacApp(p), Platform::MacosArm64) => p
            .components()
            .any(|c| c.as_os_str() == "AppTranslocation")
            .then_some(Block::Translocated),
        (Installation::AppImage(_), Platform::LinuxX86_64) => None,
        _ => Some(Block::WrongKind),
    }
}

/// Whether a running Studio may install an update it finds by itself,
/// from `STUDIO_NO_AUTO_UPDATE`'s value: unset, empty or `0` lets it;
/// anything else keeps it to checking.
#[must_use]
pub fn background_install(no_auto_update: Option<&std::ffi::OsStr>) -> bool {
    no_auto_update.is_none_or(|v| v.is_empty() || v == "0")
}

// ── The feed ────────────────────────────────────────────────────────

/// From a release list: the newest `studio-vX.Y.Z` release that is not a
/// draft (and, unless `prereleases`, not a pre-release) and carries
/// `platform`'s asset for its version and [`SUMS_ASSET`].
#[must_use]
pub fn pick(
    releases: &[GithubRelease],
    platform: Platform,
    prereleases: bool,
) -> Option<Candidate> {
    releases
        .iter()
        .filter(|r| !r.draft && (prereleases || !r.prerelease))
        .filter_map(|r| candidate(r, platform))
        .reduce(|best, c| {
            if release::newer(&c.version, &best.version) {
                c
            } else {
                best
            }
        })
}

fn candidate(release: &GithubRelease, platform: Platform) -> Option<Candidate> {
    let version = release
        .tag_name
        .strip_prefix(TAG_PREFIX)
        .filter(|v| release::is_release_version(v))?;
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
    };
    Some(Candidate {
        tag: release.tag_name.clone(),
        version: version.to_owned(),
        binary_url: url(&platform.asset(version))?,
        sums_url: url(SUMS_ASSET)?,
        sig_url: url(SIG_ASSET),
        notes_url: release.html_url.clone(),
    })
}

/// Where the releases are listed.
#[derive(Clone, Debug)]
pub struct Feed {
    /// The releases API (`https://api.github.com`, or a test's server).
    pub api: String,
    /// `owner/name`.
    pub repo: String,
    /// Pre-releases count (Studio's alpha releases are pre-releases).
    pub prereleases: bool,
}

impl Feed {
    /// Studio's releases on GitHub, pre-releases included.
    #[must_use]
    pub fn github() -> Self {
        Self {
            api: release::GITHUB_API.to_owned(),
            repo: REPO.to_owned(),
            prereleases: true,
        }
    }

    /// The newest release with `platform`'s asset, if any.
    ///
    /// # Errors
    ///
    /// The list cannot be fetched, is too large, or is not a release list.
    pub async fn newest(&self, platform: Platform) -> Result<Option<Candidate>, ReleaseError> {
        let releases = self.releases().await?;
        Ok(pick(&releases, platform, self.prereleases))
    }

    async fn releases(&self) -> Result<Vec<GithubRelease>, ReleaseError> {
        let http = |source| ReleaseError::Http {
            what: "read the release list".into(),
            source,
        };
        let client = reqwest::Client::builder()
            .user_agent("studio-self-update")
            .timeout(LIST_TIMEOUT)
            .build()
            .map_err(http)?;
        let url = format!(
            "{}/repos/{}/releases?per_page=30",
            self.api.trim_end_matches('/'),
            self.repo
        );
        let body = client
            .get(url)
            .header("accept", "application/vnd.github+json")
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(http)?
            .bytes()
            .await
            .map_err(http)?;
        if u64::try_from(body.len()).unwrap_or(u64::MAX) > MAX_LIST_BYTES {
            return Err(ReleaseError::TooLarge {
                what: "the release list".into(),
                limit: MAX_LIST_BYTES,
            });
        }
        release::parse_releases(&body)
    }
}

// ── A run ───────────────────────────────────────────────────────────

/// Check only, or install the newest release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Check,
    Install,
}

/// What a check or an install found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    /// The running version is the newest release (or newer).
    Current { newest: String },
    /// No release carries this platform's build yet.
    NoRelease,
    /// A newer release exists; nothing changed.
    Available { newest: String },
    /// A newer release exists, and this Studio cannot install it.
    Blocked { newest: String, why: Block },
    /// A newer release was verified and put in place; it runs from the
    /// next launch.
    Installed { to: String },
    /// The check or the install failed; the installation is unchanged.
    Failed(String),
}

impl UpdateStatus {
    /// `current`, `no-release`, `available`, `blocked`, `installed` or
    /// `failed`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Current { .. } => "current",
            Self::NoRelease => "no-release",
            Self::Available { .. } => "available",
            Self::Blocked { .. } => "blocked",
            Self::Installed { .. } => "installed",
            Self::Failed(_) => "failed",
        }
    }

    /// The newest release's version, when the feed named one.
    #[must_use]
    pub fn newest(&self) -> Option<&str> {
        match self {
            Self::Current { newest }
            | Self::Available { newest }
            | Self::Blocked { newest, .. } => Some(newest),
            Self::Installed { to } => Some(to),
            Self::NoRelease | Self::Failed(_) => None,
        }
    }
}

impl fmt::Display for UpdateStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Current { newest } => write!(f, "Up to date (newest release: {newest})."),
            Self::NoRelease => f.write_str("No release has a build for this platform yet."),
            Self::Available { newest } => write!(f, "Studio {newest} is available."),
            Self::Blocked { newest, why } => write!(f, "Studio {newest} is available. {why}"),
            Self::Installed { to } => write!(
                f,
                "Studio {to} is installed. Quit and reopen Studio to use it."
            ),
            Self::Failed(e) => write!(f, "The update failed: {e}"),
        }
    }
}

/// What a run updates, and with whose key.
#[derive(Clone, Copy, Debug)]
pub struct Target<'a> {
    /// The running version.
    pub current: &'a str,
    /// What is installed.
    pub installation: &'a Installation,
    /// This build's platform.
    pub platform: Option<Platform>,
    /// The release key's public half.
    pub key: Option<&'a str>,
    /// The binary is a release build.
    pub released: bool,
    /// Where a `.dmg` is downloaded to (macOS; any writable directory).
    pub scratch: &'a Path,
    /// The `studio` link and desktop entry an `AppImage` update moves.
    pub links: &'a LinuxLinks,
}

/// Check `feed` for a release newer than `target.current`, and (in
/// [`Mode::Install`]) download, verify and put it in place.
pub async fn run(feed: &Feed, target: Target<'_>, mode: Mode) -> UpdateStatus {
    let Some(platform) = target.platform else {
        return UpdateStatus::NoRelease;
    };
    let found = match feed.newest(platform).await {
        Ok(Some(found)) => found,
        Ok(None) => return UpdateStatus::NoRelease,
        Err(e) => return UpdateStatus::Failed(e.to_string()),
    };
    let newest = found.version.clone();
    if !release::newer(&newest, target.current) {
        return UpdateStatus::Current { newest };
    }
    if mode == Mode::Check {
        return UpdateStatus::Available { newest };
    }
    if let Some(why) = install_block(
        target.installation,
        target.platform,
        target.key,
        target.released,
    ) {
        return UpdateStatus::Blocked { newest, why };
    }
    match install(feed, &found, platform, target).await {
        Ok(()) => UpdateStatus::Installed { to: newest },
        Err(e) => UpdateStatus::Failed(e.to_string()),
    }
}

async fn install(
    feed: &Feed,
    found: &Candidate,
    platform: Platform,
    target: Target<'_>,
) -> Result<(), ReleaseError> {
    let asset = platform.asset(&found.version);
    let updater = Updater::new(SIGNER, &feed.repo, &asset).api_base(&feed.api);
    match target.installation {
        Installation::AppImage(image) => {
            let dir = parent(image)?;
            let staged = updater
                .fetch_verified(found, target.key, dir, &mut |_, _| {})
                .await?;
            apply_appimage(staged, image, &asset, target.links)
        }
        Installation::MacApp(bundle) => {
            std::fs::create_dir_all(target.scratch)
                .map_err(io_err(format!("create {}", target.scratch.display())))?;
            let dmg = updater
                .fetch_verified(found, target.key, target.scratch, &mut |_, _| {})
                .await?;
            mac::apply(dmg.path(), bundle, &found.version)
        }
        Installation::Unmanaged(_) => Err(ReleaseError::Usage(Block::Unmanaged.to_string())),
    }
}

fn parent(path: &Path) -> Result<&Path, ReleaseError> {
    path.parent().ok_or_else(|| ReleaseError::Io {
        what: format!("{} has no directory", path.display()),
        source: io::ErrorKind::InvalidInput.into(),
    })
}

fn io_err(what: impl Into<String>) -> impl FnOnce(io::Error) -> ReleaseError {
    let what = what.into();
    move |source| ReleaseError::Io { what, source }
}

// ── Linux: the AppImage ─────────────────────────────────────────────

/// What `scripts/install.sh` points at the `AppImage` it installs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinuxLinks {
    /// The `studio` symlink on PATH (`~/.local/bin/studio`).
    pub bin: Option<PathBuf>,
    /// The desktop entry (`~/.local/share/applications/app.fasttrackstudio.studio.desktop`).
    pub desktop: Option<PathBuf>,
}

impl LinuxLinks {
    /// Where `scripts/install.sh` puts them, from its environment
    /// (`STUDIO_BIN_DIR`, `XDG_DATA_HOME`) and `home`.
    #[must_use]
    pub fn from_env(home: Option<&Path>, bin_dir: Option<&Path>, data_home: Option<&Path>) -> Self {
        let bin_dir = bin_dir
            .map(Path::to_owned)
            .or_else(|| home.map(|h| h.join(".local/bin")));
        let data_home = data_home
            .map(Path::to_owned)
            .or_else(|| home.map(|h| h.join(".local/share")));
        Self {
            bin: bin_dir.map(|d| d.join("studio")),
            desktop: data_home.map(|d| d.join("applications/app.fasttrackstudio.studio.desktop")),
        }
    }
}

/// Where the new `AppImage` goes: beside `running` under the new version's
/// name (`asset`) when `running` carries a versioned release name, else at
/// `running` itself.
#[must_use]
pub fn appimage_destination(running: &Path, asset: &str) -> PathBuf {
    let versioned = running
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("Studio-") && n.ends_with("-linux-x86_64.AppImage"));
    match running.parent() {
        Some(dir) if versioned => dir.join(asset),
        _ => running.to_owned(),
    }
}

fn apply_appimage(
    staged: tempfile::NamedTempFile,
    running: &Path,
    asset: &str,
    links: &LinuxLinks,
) -> Result<(), ReleaseError> {
    let dest = appimage_destination(running, asset);
    release::persist_executable(staged, &dest)?;
    if dest != running {
        relink(running, &dest, links)?;
        // The running process keeps its inode; the file goes now.
        let _ = std::fs::remove_file(running);
    }
    Ok(())
}

/// Point what pointed at `old` at `new`: the `studio` symlink (when it
/// links to `old`) and the desktop entry's `Exec=` and `TryExec=` lines
/// (when they name `old`). Anything else is left alone.
///
/// # Errors
///
/// The link or the entry cannot be replaced.
pub fn relink(old: &Path, new: &Path, links: &LinuxLinks) -> Result<(), ReleaseError> {
    if let Some(bin) = &links.bin
        && std::fs::read_link(bin).is_ok_and(|to| same_file(&to, old))
    {
        let dir = parent(bin)?;
        let tmp = dir.join(format!(".studio.update-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        symlink(new, &tmp).map_err(io_err(format!("link {}", tmp.display())))?;
        std::fs::rename(&tmp, bin).map_err(io_err(format!("replace {}", bin.display())))?;
    }
    if let Some(entry) = &links.desktop
        && let Ok(text) = std::fs::read_to_string(entry)
        && let Some(moved) = desktop_entry_moved(&text, old, new)
    {
        let dir = parent(entry)?;
        let name = entry.file_name().map_or_else(
            || "studio.desktop".into(),
            |n| n.to_string_lossy().into_owned(),
        );
        release::write_atomic(dir, &name, &moved)?;
    }
    Ok(())
}

/// `text` (a desktop entry) with its `Exec=`/`TryExec=` program moved
/// from `old` to `new`; `None` when no such line names `old`.
#[must_use]
pub fn desktop_entry_moved(text: &str, old: &Path, new: &Path) -> Option<String> {
    let (old, new) = (old.to_str()?, new.to_str()?);
    let mut changed = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let moved = ["Exec=", "TryExec="].iter().find_map(|key| {
            let rest = line.strip_prefix(key)?;
            let tail = rest.strip_prefix(old)?;
            tail.chars()
                .next()
                .is_none_or(char::is_whitespace)
                .then(|| format!("{key}{new}{tail}"))
        });
        if let Some(moved) = moved {
            changed = true;
            out.push_str(&moved);
        } else {
            out.push_str(line);
        }
    }
    changed.then_some(out)
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

#[cfg(unix)]
fn symlink(to: &Path, at: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(to, at)
}

#[cfg(not(unix))]
fn symlink(_to: &Path, _at: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

// ── macOS: the bundle ───────────────────────────────────────────────

/// Put the bundle `new` (a verified copy on `bundle`'s volume) where
/// `bundle` is.
///
/// `bundle` is moved into `trash` (a directory on the same volume, which
/// the caller removes), then `new` is renamed to `bundle`. If that second
/// rename fails the old bundle is moved back.
///
/// # Errors
///
/// Either rename fails; `bundle` is then where it was.
pub fn swap_bundle(new: &Path, bundle: &Path, trash: &Path) -> io::Result<()> {
    let old = trash.join("previous.app");
    std::fs::rename(bundle, &old)?;
    if let Err(e) = std::fs::rename(new, bundle) {
        let _ = std::fs::rename(&old, bundle);
        return Err(e);
    }
    Ok(())
}

/// The `TeamIdentifier=` that `codesign -dv` printed, when it is set.
#[must_use]
pub fn team_identifier(codesign_display: &str) -> Option<&str> {
    codesign_display
        .lines()
        .find_map(|l| l.strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|t| !t.is_empty() && *t != "not set")
}

mod mac {
    use std::path::Path;
    use std::process::{Command, Output};

    use architect::release::ReleaseError;

    use super::{BUNDLE, BUNDLE_PROGRAM, SIGNER, io_err, parent, swap_bundle, team_identifier};

    /// Mount `dmg`, check its `Studio.app` and swap it in for `bundle`.
    pub(super) fn apply(dmg: &Path, bundle: &Path, version: &str) -> Result<(), ReleaseError> {
        let scratch = parent(dmg)?;
        let mount = tempfile::Builder::new()
            .prefix("mount-")
            .tempdir_in(scratch)
            .map_err(io_err("make a mount point"))?;
        attach(dmg, mount.path())?;
        let result = install_from(&mount.path().join(BUNDLE), bundle, version);
        detach(mount.path());
        result
    }

    fn install_from(new: &Path, bundle: &Path, version: &str) -> Result<(), ReleaseError> {
        if !new.is_dir() {
            return Err(refused(format!("the .dmg holds no {BUNDLE}")));
        }
        check_signature(new, bundle)?;
        let said = run(Command::new(new.join(BUNDLE_PROGRAM)).arg("--version"))
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
        let expected = format!("{SIGNER} {version}");
        if said.as_deref() != Some(expected.as_str()) {
            return Err(ReleaseError::SmokeTest {
                said: said.unwrap_or_else(|| "nothing (it failed)".into()),
                expected,
            });
        }
        // A copy beside the bundle, so the swap is a rename on one volume.
        let staging = tempfile::Builder::new()
            .prefix(".Studio.app.update-")
            .tempdir_in(parent(bundle)?)
            .map_err(io_err(format!(
                "make room beside {} (is its folder writable by this user?)",
                bundle.display()
            )))?;
        let copy = staging.path().join(BUNDLE);
        // ditto keeps the code signature and the stapled ticket.
        let out = run(Command::new("/usr/bin/ditto").arg(new).arg(&copy))
            .map_err(io_err("copy the new Studio.app"))?;
        if !out.status.success() {
            return Err(refused(format!("ditto failed: {}", stderr(&out))));
        }
        swap_bundle(&copy, bundle, staging.path())
            .map_err(io_err(format!("replace {}", bundle.display())))
        // `staging` (with the old bundle in it) is removed as it drops.
    }

    /// `new` must pass a strict signature check and, when the running
    /// bundle is signed by a team, be signed by the same one.
    fn check_signature(new: &Path, running: &Path) -> Result<(), ReleaseError> {
        let verify = run(Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(new))
        .map_err(io_err("run codesign"))?;
        if !verify.status.success() {
            return Err(refused(format!(
                "its code signature does not verify: {}",
                stderr(&verify)
            )));
        }
        let team = |p: &Path| {
            run(Command::new("/usr/bin/codesign")
                .args(["-dv", "--verbose=2"])
                .arg(p))
            .ok()
            .and_then(|o| team_identifier(&String::from_utf8_lossy(&o.stderr)).map(str::to_owned))
        };
        if let Some(ours) = team(running)
            && team(new).as_deref() != Some(ours.as_str())
        {
            return Err(refused(format!(
                "it is not signed by this Studio's team ({ours})"
            )));
        }
        Ok(())
    }

    fn attach(dmg: &Path, mount: &Path) -> Result<(), ReleaseError> {
        // diskutil image on macOS 26+ (hdiutil attach is deprecated there),
        // hdiutil before, as scripts/install.sh does.
        let diskutil = run(Command::new("/usr/sbin/diskutil")
            .args([
                "image",
                "attach",
                "--mountOptions",
                "nobrowse",
                "--readOnly",
                "--mountPoint",
            ])
            .arg(mount)
            .arg(dmg));
        if diskutil.is_ok_and(|o| o.status.success()) {
            return Ok(());
        }
        let out = run(Command::new("/usr/bin/hdiutil")
            .args([
                "attach",
                "-nobrowse",
                "-readonly",
                "-noautoopen",
                "-mountpoint",
            ])
            .arg(mount)
            .arg(dmg))
        .map_err(io_err("run hdiutil"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(refused(format!("the .dmg does not open: {}", stderr(&out))))
        }
    }

    fn detach(mount: &Path) {
        let plain = run(Command::new("/usr/bin/hdiutil").arg("detach").arg(mount));
        if !plain.is_ok_and(|o| o.status.success()) {
            let _ = run(Command::new("/usr/bin/hdiutil")
                .args(["detach", "-force"])
                .arg(mount));
        }
    }

    fn run(cmd: &mut Command) -> std::io::Result<Output> {
        cmd.stdin(std::process::Stdio::null()).output()
    }

    fn stderr(out: &Output) -> String {
        String::from_utf8_lossy(&out.stderr).trim().to_owned()
    }

    fn refused(reason: impl std::fmt::Display) -> ReleaseError {
        ReleaseError::Usage(format!(
            "the new {BUNDLE} was refused: {reason}; nothing changed"
        ))
    }
}

// ── The app's skin ──────────────────────────────────────────────────

/// The running Studio, as the app names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct App {
    /// The running version (`fts_version::release_version()`).
    pub version: &'static str,
    /// Built as a release (`fts_version::RELEASED`).
    pub released: bool,
}

impl App {
    #[must_use]
    pub const fn new(version: &'static str, released: bool) -> Self {
        Self { version, released }
    }

    /// How the running Studio is installed.
    #[must_use]
    pub fn installation(&self) -> Installation {
        let exe = release::current_exe().unwrap_or_default();
        let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        Installation::detect(&exe, appimage.as_deref())
    }

    /// Why this Studio cannot install an update, if it cannot.
    #[must_use]
    pub fn block(&self) -> Option<Block> {
        install_block(
            &self.installation(),
            Platform::current(),
            RELEASE_KEY,
            self.released,
        )
    }

    /// Check (or install the newest release from) `feed`
    /// ([`Feed::github`] in a shipped Studio). On a thread of its own, so
    /// any executor can await it.
    pub async fn update(self, feed: Feed, mode: Mode) -> UpdateStatus {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("studio-update".into())
            .spawn(move || {
                let _ = tx.send(self.blocking(&feed, mode));
            });
        if let Err(e) = spawned {
            return UpdateStatus::Failed(format!("could not start the update: {e}"));
        }
        rx.await
            .unwrap_or_else(|_| UpdateStatus::Failed("the update stopped".into()))
    }

    fn blocking(self, feed: &Feed, mode: Mode) -> UpdateStatus {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => return UpdateStatus::Failed(format!("no runtime for the update: {e}")),
        };
        let installation = self.installation();
        let home = dirs::home_dir();
        let env_path = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let links = LinuxLinks::from_env(
            home.as_deref(),
            env_path("STUDIO_BIN_DIR").as_deref(),
            env_path("XDG_DATA_HOME").as_deref(),
        );
        let scratch = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("app.fasttrackstudio.studio/updates");
        let target = Target {
            current: self.version,
            installation: &installation,
            platform: Platform::current(),
            key: RELEASE_KEY,
            released: self.released,
            scratch: &scratch,
            links: &links,
        };
        runtime.block_on(run(feed, target, mode))
    }
}
