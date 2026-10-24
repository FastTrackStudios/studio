//! The update check and install, on `architect::release`.
//!
//! The newest `vX.Y.Z` GitHub Release of the studio repo that carries this
//! platform's binary, its `SHA256SUMS` and the sums' minisign signature by
//! the org release key.
//!
//! The pieces that decide are honest ([`run`], [`UpdateStatus::from_run`],
//! [`install_block`]) and take the feed, the binary and the key as
//! parameters, so a test drives them against a fake feed. The apps' Settings
//! call the skin: [`App::check`] / [`App::install`] (which read the running
//! binary, the compiled-in key and the environment, and run the network on
//! a thread of their own) and [`App::install_block`]; with feature `ui`,
//! `UpdateSettings` is the Settings block both apps mount.
//!
//! What is not handled here: a macOS `.app` bundle (its code signature and
//! Gatekeeper; the `.dmg` replaces it) and an `AppImage` (a read-only mount):
//! both are reported by [`install_block`] instead of attempted.

use std::fmt;
use std::path::Path;

use architect::release::{self, Options, Outcome, ReleaseError, Target, Updater};

#[cfg(feature = "ui")]
mod view;
#[cfg(feature = "ui")]
pub use view::UpdateSettings;

/// Where Session and Signal are released: one `vX.Y.Z` tag, one GitHub
/// Release, a binary per app and platform.
pub const REPO: &str = "FastTrackStudios/studio";

/// The org release key's public half: the base64 line of its minisign
/// `.pub` (one org-wide key, Captain decision 1 in STA-381). The one place
/// it lives.
///
/// Set at build time with `FTS_RELEASE_PUBKEY=RW…`; otherwise
/// [`ORG_RELEASE_KEY`]. `None` is "this build pins no key": checks still
/// work and an install is refused (`NoSigningKey`), never skipped.
pub const RELEASE_KEY: Option<&str> = match option_env!("FTS_RELEASE_PUBKEY") {
    Some(key) => Some(key),
    None => ORG_RELEASE_KEY,
};

/// The org key compiled into every build. Unset until the Captain
/// publishes the key: paste the `.pub` file's base64 line as `Some("RW…")`.
const ORG_RELEASE_KEY: Option<&str> = None;

/// One app's releases, as it names itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct App {
    /// The binary's name, as its `--version` prints it (`<name> <version>`)
    /// and the release asset is named (`<name>-<os>-<arch>`): the app's
    /// `env!("CARGO_PKG_NAME")`.
    pub name: &'static str,
    /// The running version: the app's `env!("CARGO_PKG_VERSION")`.
    pub version: &'static str,
}

/// What a check or an install found, for Settings to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    /// The running version is the newest release (or newer).
    Current { newest: String },
    /// No release carries this platform's binary yet.
    NoRelease,
    /// A newer release exists; nothing changed.
    Available { newest: String },
    /// A newer binary was verified and swapped in; it runs from the next
    /// launch.
    Installed { to: String },
    /// The check or the install failed; the installed binary is unchanged
    /// (except after a failing hook, which Settings never sets).
    Failed(String),
}

/// Check only, or install the newest release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Check,
    Install,
}

/// Why this binary cannot replace itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallBlock {
    /// The build pins no release key ([`RELEASE_KEY`] is `None`).
    NoKey,
    /// The binary is inside a macOS `.app` bundle; swapping it would break
    /// the bundle's signature.
    AppBundle,
    /// The binary runs from an `AppImage`'s read-only mount.
    AppImage,
}

impl App {
    /// The app built as this crate: `App::new(env!("CARGO_PKG_NAME"),
    /// env!("CARGO_PKG_VERSION"))`.
    #[must_use]
    pub const fn new(name: &'static str, version: &'static str) -> Self {
        Self { name, version }
    }

    /// Its releases in [`REPO`], this platform's asset.
    #[must_use]
    pub fn updater(&self) -> Updater {
        Updater::new(self.name, REPO, release::platform_asset(self.name))
    }

    /// Check the feed. Runs on a thread of its own, so it can be awaited
    /// from any UI executor.
    pub async fn check(self) -> UpdateStatus {
        self.off_ui(Mode::Check).await
    }

    /// Download, verify and swap in the newest release. Runs on a thread
    /// of its own, like [`App::check`].
    pub async fn install(self) -> UpdateStatus {
        self.off_ui(Mode::Install).await
    }

    /// Why this running binary cannot install an update, if it cannot.
    #[must_use]
    pub fn install_block(&self) -> Option<InstallBlock> {
        let exe = release::current_exe().ok()?;
        let appimage = std::env::var_os("APPIMAGE").is_some();
        install_block(&exe, RELEASE_KEY, appimage)
    }

    async fn off_ui(self, mode: Mode) -> UpdateStatus {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("app-shell-update".into())
            .spawn(move || {
                let _ = tx.send(self.blocking(mode));
            });
        if let Err(e) = spawned {
            return UpdateStatus::Failed(format!("could not start the update check: {e}"));
        }
        rx.await
            .unwrap_or_else(|_| UpdateStatus::Failed("the update check stopped".into()))
    }

    fn blocking(self, mode: Mode) -> UpdateStatus {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => return UpdateStatus::Failed(format!("no runtime for the update check: {e}")),
        };
        let exe = match release::current_exe() {
            Ok(exe) => exe,
            Err(e) => return UpdateStatus::from_run(Err(e)),
        };
        let target = Target {
            current: self.version,
            exe: &exe,
            key: RELEASE_KEY,
        };
        runtime.block_on(run(&self.updater(), target, mode))
    }
}

/// Check (or install) `target` against `updater`'s feed. Honest: the feed,
/// the binary and the key all come in as parameters.
pub async fn run(updater: &Updater, target: Target<'_>, mode: Mode) -> UpdateStatus {
    let opts = Options {
        check: mode == Mode::Check,
        ..Options::default()
    };
    UpdateStatus::from_run(updater.run(target, &opts).await)
}

/// Why a binary at `exe` cannot replace itself with a release signed by
/// `key`, if it cannot. `appimage`: the process runs from an `AppImage` (its
/// `APPIMAGE` variable is set).
#[must_use]
pub fn install_block(exe: &Path, key: Option<&str>, appimage: bool) -> Option<InstallBlock> {
    if key.is_none() {
        return Some(InstallBlock::NoKey);
    }
    if exe
        .ancestors()
        .any(|p| p.extension().is_some_and(|ext| ext == "app"))
    {
        return Some(InstallBlock::AppBundle);
    }
    appimage.then_some(InstallBlock::AppImage)
}

impl UpdateStatus {
    /// What an `architect::release` run's result means to Settings.
    #[must_use]
    pub fn from_run(result: Result<Outcome, ReleaseError>) -> Self {
        match result {
            Ok(Outcome::NoRelease { .. }) => Self::NoRelease,
            Ok(Outcome::Current { newest, .. }) => Self::Current { newest },
            Ok(Outcome::AlreadyInstalled { current }) => Self::Current { newest: current },
            Ok(Outcome::Available { newest, .. }) => Self::Available { newest },
            Ok(Outcome::Installed { to, .. }) => Self::Installed { to },
            Err(e) => Self::Failed(e.to_string()),
        }
    }

    /// The version to offer an install of, if there is one.
    #[must_use]
    pub fn available(&self) -> Option<&str> {
        match self {
            Self::Available { newest } => Some(newest),
            _ => None,
        }
    }
}

impl fmt::Display for UpdateStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Current { newest } => write!(f, "Up to date (newest release: v{newest})."),
            Self::NoRelease => f.write_str("No release has a build for this platform yet."),
            Self::Available { newest } => write!(f, "Update available: v{newest}."),
            Self::Installed { to } => write!(f, "Updated to v{to}. Restart to use it."),
            Self::Failed(e) => write!(f, "Update check failed: {e}"),
        }
    }
}

impl fmt::Display for InstallBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoKey => "This build pins no release key, so it can check but not install.",
            Self::AppBundle => "Install the new version from its .dmg.",
            Self::AppImage => "Download the new AppImage to update.",
        })
    }
}
