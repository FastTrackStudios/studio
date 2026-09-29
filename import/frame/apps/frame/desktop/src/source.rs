//! Where the surface comes from, and noticing when it changes on disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use surface_runtime::Surface;
use surface_widgets::{Options, Theme, surface_for_catalog};

#[derive(Debug, Clone)]
pub enum Source {
    /// The surface generated from the demo engine's catalog.
    Demo { theme: Box<Theme> },
    /// frame markup text (`.fm`).
    Markup(PathBuf),
    /// A `.frame` bundle.
    Bundle(PathBuf),
}

impl Source {
    #[must_use]
    pub fn from_path(path: PathBuf) -> Self {
        if path.extension().is_some_and(|e| e == "frame") { Self::Bundle(path) } else { Self::Markup(path) }
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Demo { .. } => None,
            Self::Markup(p) | Self::Bundle(p) => Some(p),
        }
    }

    /// A load error with the path shortened to the file name, for the
    /// in-window banner (stderr keeps the full, clickable path).
    #[must_use]
    pub fn short_error(&self, error: &str) -> String {
        let Some(path) = self.path() else {
            return error.to_owned();
        };
        let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        error
            .strip_prefix(&*path.display().to_string())
            .map_or_else(|| error.to_owned(), |rest| format!("{name}{rest}"))
    }

    /// Build the surface. Errors are one line, ready for stderr and the
    /// in-window banner: `path:line:col: message` for markup.
    ///
    /// # Errors
    /// Unreadable file, markup parse error, or a broken bundle.
    pub fn load(&self) -> Result<Surface, String> {
        match self {
            Self::Demo { theme } => {
                let catalog = surface_memory::demo_catalog();
                Ok(surface_for_catalog(&catalog, &Options { theme: (**theme).clone(), ..Options::default() }))
            }
            Self::Markup(path) => {
                let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
                let scene =
                    document_text::parse(&src, BTreeMap::new()).map_err(|e| format!("{}:{e}", path.display()))?;
                Ok(Surface::from_scene(scene))
            }
            Self::Bundle(path) => {
                let (_, doc) = document_bundle::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(Surface::from_scene(doc.scene()))
            }
        }
    }
}

/// An empty dark surface to show (with the error banner) when the file is
/// broken at startup, so the window still opens and waits for a fix.
#[must_use]
pub fn placeholder() -> Surface {
    const SRC: &str = r##"<Document><Page><Frame w=960 h=360 fill="#0e0f12" /></Page></Document>"##;
    Surface::from_scene(document_text::parse(SRC, BTreeMap::new()).expect("placeholder markup is valid"))
}

/// How often to look at the file.
pub const POLL: Duration = Duration::from_millis(250);

/// A file's identity for change detection: mtime plus length, so a
/// same-second rewrite on a coarse-mtime filesystem still registers.
pub type Stamp = Option<(SystemTime, u64)>;

#[must_use]
pub fn stamp(path: &Path) -> Stamp {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// Polls a stamp and says when to reload.
#[derive(Debug, Clone)]
pub struct Watch {
    last: Stamp,
    next: Instant,
}

impl Watch {
    #[must_use]
    pub fn new(initial: Stamp, now: Instant) -> Self {
        Self { last: initial, next: now + POLL }
    }

    /// When the next check is due.
    #[must_use]
    pub fn deadline(&self) -> Instant {
        self.next
    }

    /// Whether a check is due at `now`.
    #[must_use]
    pub fn due(&self, now: Instant) -> bool {
        now >= self.next
    }

    /// Record the stamp seen at `now`; true when the file changed and
    /// exists (a vanished file — mid-save rename — waits for it to return).
    pub fn observe(&mut self, current: Stamp, now: Instant) -> bool {
        self.next = now + POLL;
        if current == self.last {
            return false;
        }
        self.last = current;
        current.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64, len: u64) -> Stamp {
        Some((SystemTime::UNIX_EPOCH + Duration::from_secs(secs), len))
    }

    #[test]
    fn reloads_only_on_change() {
        let t0 = Instant::now();
        let mut w = Watch::new(at(1, 10), t0);
        assert!(!w.due(t0));
        assert!(w.due(t0 + POLL));
        assert!(!w.observe(at(1, 10), t0 + POLL));
        assert!(w.observe(at(2, 10), t0 + POLL * 2), "new mtime");
        assert!(!w.observe(at(2, 10), t0 + POLL * 3), "reported once");
        assert!(w.observe(at(2, 11), t0 + POLL * 4), "same mtime, new length");
    }

    #[test]
    fn vanished_file_waits_for_it_to_return() {
        let t0 = Instant::now();
        let mut w = Watch::new(at(1, 10), t0);
        assert!(!w.observe(None, t0));
        assert!(!w.observe(None, t0));
        assert!(w.observe(at(1, 10), t0), "back (even unchanged) → reload");
    }

    #[test]
    fn observe_schedules_the_next_check() {
        let t0 = Instant::now();
        let mut w = Watch::new(None, t0);
        w.observe(None, t0 + POLL * 3);
        assert_eq!(w.deadline(), t0 + POLL * 4);
    }

    #[test]
    fn stamps_a_real_file() {
        let dir = std::env::temp_dir().join(format!("frame-desktop-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.fm");
        assert_eq!(stamp(&f), None);
        std::fs::write(&f, "abc").unwrap();
        assert_eq!(stamp(&f).map(|s| s.1), Some(3));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn placeholder_has_a_page() {
        assert_eq!(placeholder().scene.pages().len(), 1);
    }

    #[test]
    fn picks_loader_by_extension() {
        assert!(matches!(Source::from_path("a/b.frame".into()), Source::Bundle(_)));
        assert!(matches!(Source::from_path("a/b.fm".into()), Source::Markup(_)));
    }

    #[test]
    fn markup_errors_carry_path_line_and_col() {
        let dir = std::env::temp_dir().join(format!("frame-desktop-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("bad.fm");
        std::fs::write(&f, "<Document>\n  <Page>\n</Document>").unwrap();
        let err = Source::Markup(f.clone()).load().unwrap_err();
        assert!(err.starts_with(&format!("{}:", f.display())), "{err}");
        assert!(err.split(':').nth(1).is_some_and(|l| l.parse::<usize>().is_ok()), "{err}");
        assert!(Source::Markup(f).short_error(&err).starts_with("bad.fm:"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
