//! A theme a running app is drawn with: loaded from files, hot-reloaded,
//! edited, saved and reverted, and never broken by a bad edit.
//!
//! [`ThemeHost`] is what a host keeps beside its views. It holds the
//! layered files a theme is ([`Theme::from_sources`]'s layers: base first,
//! each a file or a folder of `.frame`/`.fm` files) and the theme they
//! compiled to. A view draws [`ThemeHost::current`] and re-records when
//! [`ThemeHost::version`] moves.
//!
//! **Last good.** Every change — a file changed on disk ([`ThemeHost::poll`]),
//! an edit ([`ThemeHost::edit`]), a reload, another theme — compiles first
//! and replaces the theme only when it compiled. One that did not leaves
//! the last good theme on screen, and says why: every problem, each a
//! [`ThemeError`] with its file, line, role and node as far as they are
//! known ([`ThemeHost::errors`]).
//!
//! **Drafts.** An edit is held in memory over its file until
//! [`ThemeHost::save`] writes it. [`ThemeHost::revert`] puts every file back
//! byte for byte as it was when the theme was loaded, and drops the drafts.
//!
//! **Threads.** The host is `Send + Sync`. Drawing reads [`ThemeHost::current`]
//! (an `Arc` behind a read lock, swapped once a new theme has compiled), so a
//! view never waits for a compile: compiling on a worker thread is calling
//! [`ThemeHost::poll`] there. Nothing here runs on, or blocks, an audio thread.
//!
//! **No clock.** [`ThemeHost::poll`] looks at the files every time it is
//! called; the host calls it on its own clock, about every [`POLL`].

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{Duration, SystemTime};

use facet::Facet;

use crate::theme::runtime::{Diagnostic, Schema, Theme};

/// How often a host should [`ThemeHost::poll`]: Studio's cadence.
pub const POLL: Duration = Duration::from_millis(250);

/// What a problem with a theme is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ErrorKind {
    /// There are no theme files to load.
    NoFiles,
    /// A file could not be read or written.
    Io,
    /// A file is not a frame document.
    Parse,
    /// The documents parse, and together are not a theme for this host:
    /// an undefined token, a field the host does not have, a bad binding.
    Compile,
    /// The call itself: a file that is not one of the theme's.
    Request,
}

/// Something wrong with a theme, located as far as it can be.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ThemeError {
    pub kind: ErrorKind,
    /// The file it is in.
    pub file: Option<String>,
    /// Where in the file, 1-based (a parse error).
    pub line: Option<u32>,
    pub col: Option<u32>,
    /// The role (page) it is in.
    pub role: Option<String>,
    /// The node's name.
    pub node: Option<String>,
    pub message: String,
}

impl ThemeError {
    fn new(kind: ErrorKind, file: Option<&Path>, message: impl Into<String>) -> Self {
        Self {
            kind,
            file: file.map(|f| f.display().to_string()),
            line: None,
            col: None,
            role: None,
            node: None,
            message: message.into(),
        }
    }

    fn compile(d: Diagnostic) -> Self {
        Self {
            role: d.role,
            node: d.node,
            ..Self::new(ErrorKind::Compile, None, d.message)
        }
    }
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            f.write_str(file)?;
            if let (Some(l), Some(c)) = (self.line, self.col) {
                write!(f, ":{l}:{c}")?;
            }
            f.write_str(": ")?;
        }
        match (&self.role, &self.node) {
            (Some(r), Some(n)) => write!(f, "{r} › {n}: {}", self.message),
            (Some(r), None) => write!(f, "{r}: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

/// What [`ThemeHost::poll`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reload {
    /// The files changed and compiled: [`ThemeHost::current`] is new.
    Applied,
    /// The files changed and did not compile: the last good theme stays.
    /// Told once per distinct set of problems.
    Kept(Vec<ThemeError>),
}

/// One of a theme's files, as [`ThemeHost::inspect`] shows it.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ThemeFile {
    pub path: String,
    /// The bytes the theme is compiled from: the draft's when there is one.
    pub bytes: u64,
    /// Their BLAKE3, hex.
    pub blake3: String,
    /// Whether an unsaved edit is held over the file.
    pub draft: bool,
}

/// One role the theme draws.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ThemeRole {
    pub name: String,
    /// The size it was drawn at.
    pub width: f64,
    pub height: f64,
    /// Whether it reads live data.
    pub live: bool,
}

/// One token and its value in the theme's mode (`#rrggbbaa` for a colour).
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ThemeToken {
    pub name: String,
    pub value: String,
}

/// The theme a host has, and what it was made from.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ThemeInfo {
    /// Whether there is a theme to draw with.
    pub loaded: bool,
    /// [`ThemeHost::version`].
    pub version: u64,
    /// The layers as given, base first.
    pub paths: Vec<String>,
    /// The mode asked for; the theme's own when none.
    pub mode: Option<String>,
    /// The documents' modes.
    pub modes: Vec<String>,
    pub roles: Vec<ThemeRole>,
    pub tokens: Vec<ThemeToken>,
    /// The files the layers come to, in layering order.
    pub files: Vec<ThemeFile>,
    /// Why the files as they are did not compile; empty when they did.
    pub errors: Vec<ThemeError>,
}

/// A file and when it last changed.
type Stamp = Vec<(PathBuf, Option<SystemTime>)>;

#[derive(Default)]
struct Source {
    paths: Vec<PathBuf>,
    mode: Option<String>,
    stamp: Stamp,
    /// Unsaved edits, over their files.
    drafts: BTreeMap<PathBuf, Draft>,
    /// Every file's bytes when the theme was loaded: what revert restores.
    baseline: BTreeMap<PathBuf, Vec<u8>>,
    /// Why the files did not compile, last time they were compiled.
    errors: Vec<ThemeError>,
}

struct Draft {
    markup: String,
    original: Vec<u8>,
}

/// A host's theme: see the [module](self).
pub struct ThemeHost {
    schema: Schema,
    theme: RwLock<Option<Arc<Theme>>>,
    version: AtomicU64,
    source: Mutex<Source>,
}

impl fmt::Debug for ThemeHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThemeHost")
            .field("version", &self.version())
            .field("theme", &self.current())
            .finish_non_exhaustive()
    }
}

impl ThemeHost {
    /// A host with no theme yet, whose themes compile against `schema`.
    #[must_use]
    pub fn new(schema: Schema) -> Self {
        Self {
            schema,
            theme: RwLock::new(None),
            version: AtomicU64::new(0),
            source: Mutex::new(Source::default()),
        }
    }

    /// The schema themes compile against.
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// The theme to draw with, when there is one. Never waits for a compile.
    #[must_use]
    pub fn current(&self) -> Option<Arc<Theme>> {
        self.theme
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The displayed theme and its publication version from one read lock.
    #[must_use]
    pub fn snapshot(&self) -> Option<(Arc<Theme>, u64)> {
        let current = self.theme.read().unwrap_or_else(PoisonError::into_inner);
        current
            .as_ref()
            .map(|theme| (theme.clone(), self.version.load(Ordering::Acquire)))
    }

    /// Bumped whenever [`Self::current`] changes: what a recording compares
    /// to know it is stale.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Draw with `theme` (or none) from now on, whatever the files say.
    pub fn set(&self, theme: Option<Arc<Theme>>) {
        let mut current = self.theme.write().unwrap_or_else(PoisonError::into_inner);
        *current = theme;
        self.version.fetch_add(1, Ordering::AcqRel);
    }

    /// Publish only if no theme has been published since `expected_version`.
    /// Returns false when a newer publication has superseded the caller.
    pub fn set_if_version(&self, expected_version: u64, theme: Arc<Theme>) -> bool {
        let mut current = self.theme.write().unwrap_or_else(PoisonError::into_inner);
        if self.version.load(Ordering::Acquire) != expected_version {
            return false;
        }
        *current = Some(theme);
        self.version.fetch_add(1, Ordering::AcqRel);
        true
    }

    /// Why the last compile failed — of the files, or of an edit to them;
    /// empty when it did not.
    #[must_use]
    pub fn errors(&self) -> Vec<ThemeError> {
        self.lock().errors.clone()
    }

    fn lock(&self) -> MutexGuard<'_, Source> {
        self.source.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Load the theme layered from `paths` (base first; files, or folders
    /// of them) in `mode` (`None`: the documents' first), and watch them.
    /// The files as they are now become what [`Self::revert`] restores;
    /// drafts of the theme before are dropped.
    ///
    /// All or nothing when there is a theme: one that does not compile
    /// leaves the theme, its files, drafts and revert point as they were.
    /// With none yet, its files are watched all the same, so the theme a
    /// host starts with broken is picked up once it is fixed.
    ///
    /// # Errors
    /// Every problem, when it did not compile; the previous theme stays.
    pub fn load(&self, paths: Vec<PathBuf>, mode: Option<String>) -> Result<(), Vec<ThemeError>> {
        let mut source = self.lock();
        let stamp = stamp(&paths);
        let baseline = stamp
            .iter()
            .filter_map(|(f, _)| std::fs::read(f).ok().map(|b| (f.clone(), b)))
            .collect();
        let mut next = Source {
            paths,
            mode,
            stamp,
            drafts: BTreeMap::new(),
            baseline,
            errors: Vec::new(),
        };
        match compile(&next, &self.schema, None) {
            Ok(theme) => {
                *source = next;
                self.set(Some(Arc::new(theme)));
                Ok(())
            }
            Err(errors) => {
                // With a theme on screen, its files stay the ones watched;
                // with none yet, these are, so fixing them is picked up.
                if self.current().is_some() {
                    source.errors.clone_from(&errors);
                } else {
                    next.errors.clone_from(&errors);
                    *source = next;
                }
                Err(errors)
            }
        }
    }

    /// Recompile from the files (and drafts) now, changed or not.
    ///
    /// # Errors
    /// Every problem, when it did not compile; the last good theme stays.
    pub fn reload(&self) -> Result<(), Vec<ThemeError>> {
        let mut source = self.lock();
        source.stamp = stamp(&source.paths);
        self.apply(&mut source)
    }

    /// Recompile if any of the files changed since the last look, or a
    /// folder gained or lost one. `None` when nothing changed, or the files
    /// fail the same way they did last time.
    pub fn poll(&self) -> Option<Reload> {
        let mut source = self.lock();
        if source.paths.is_empty() {
            return None;
        }
        let now = stamp(&source.paths);
        if now == source.stamp {
            return None;
        }
        source.stamp = now;
        let before = source.errors.clone();
        match self.apply(&mut source) {
            Ok(()) => Some(Reload::Applied),
            Err(errors) if errors == before => None,
            Err(errors) => Some(Reload::Kept(errors)),
        }
    }

    /// Compile what the files and drafts say now; use it when it compiled.
    fn apply(&self, source: &mut Source) -> Result<(), Vec<ThemeError>> {
        match compile(source, &self.schema, None) {
            Ok(theme) => {
                source.errors.clear();
                self.set(Some(Arc::new(theme)));
                Ok(())
            }
            Err(errors) => {
                source.errors.clone_from(&errors);
                Err(errors)
            }
        }
    }

    /// Replace `file` (one of the theme's) with `markup`, unsaved, and use
    /// the theme it makes. All or nothing: when it does not compile, the
    /// draft before it (or the file) and the last good theme stay.
    ///
    /// # Errors
    /// Every problem the edit has.
    pub fn edit(&self, file: &Path, markup: String) -> Result<(), Vec<ThemeError>> {
        let mut source = self.lock();
        let Some(file) = source
            .stamp
            .iter()
            .map(|(f, _)| f.clone())
            .find(|f| same_file(f, file))
        else {
            return Err(vec![ThemeError::new(
                ErrorKind::Request,
                Some(file),
                "not one of the theme's files (THEME_INSPECT lists them)",
            )]);
        };
        let edited = BTreeMap::from([(file.clone(), markup.clone())]);
        let theme = compile(&source, &self.schema, Some(&edited)).inspect_err(|e| {
            source.errors.clone_from(e);
        })?;
        let original = match source.drafts.get(&file) {
            Some(draft) => draft.original.clone(),
            None => std::fs::read(&file)
                .map_err(|e| vec![ThemeError::new(ErrorKind::Io, Some(&file), e.to_string())])?,
        };
        source.drafts.insert(file, Draft { markup, original });
        source.errors.clear();
        self.set(Some(Arc::new(theme)));
        Ok(())
    }

    /// Write every draft to its file. Answers the files written.
    ///
    /// # Errors
    /// The files that could not be written; their drafts stay.
    pub fn save(&self) -> Result<Vec<String>, Vec<ThemeError>> {
        let mut source = self.lock();
        let mut written = Vec::new();
        let mut errors = Vec::new();
        for (file, draft) in std::mem::take(&mut source.drafts) {
            let result = std::fs::read(&file).and_then(|bytes| {
                if bytes != draft.original {
                    return Err(std::io::Error::other(
                        "file changed since the draft was started",
                    ));
                }
                std::fs::write(&file, draft.markup.as_bytes())
            });
            match result {
                Ok(()) => written.push(file.display().to_string()),
                Err(e) => {
                    errors.push(ThemeError::new(ErrorKind::Io, Some(&file), e.to_string()));
                    source.drafts.insert(file, draft);
                }
            }
        }
        // What was written is what is compiled: no reload for it.
        let now = stamp(&source.paths);
        for (file, modified) in &mut source.stamp {
            if written.iter().any(|path| Path::new(path) == file)
                && let Some((_, updated)) = now.iter().find(|(path, _)| path == file)
            {
                *modified = *updated;
            }
        }
        if errors.is_empty() {
            Ok(written)
        } else {
            Err(errors)
        }
    }

    /// Put every file back as it was when the theme was loaded, byte for
    /// byte, drop the drafts and use the theme they make. Answers the files
    /// rewritten.
    ///
    /// # Errors
    /// The files that could not be written, or why what they are now does
    /// not compile (the last good theme stays).
    pub fn revert(&self) -> Result<Vec<String>, Vec<ThemeError>> {
        let mut source = self.lock();
        let mut written = Vec::new();
        let mut errors = Vec::new();
        for (file, bytes) in &source.baseline {
            if std::fs::read(file).is_ok_and(|now| now == *bytes) {
                continue;
            }
            match std::fs::write(file, bytes) {
                Ok(()) => written.push(file.display().to_string()),
                Err(e) => errors.push(ThemeError::new(ErrorKind::Io, Some(file), e.to_string())),
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        source.drafts.clear();
        source.stamp = stamp(&source.paths);
        self.apply(&mut source).map(|()| written)
    }

    /// The theme, its files and why they do not compile, if they do not.
    #[must_use]
    pub fn inspect(&self) -> ThemeInfo {
        let source = self.lock();
        let theme = self.current();
        let files = source
            .stamp
            .iter()
            .map(|(f, _)| {
                let draft = source.drafts.get(f);
                let bytes = match draft {
                    Some(d) => d.markup.as_bytes().to_vec(),
                    None => std::fs::read(f).unwrap_or_default(),
                };
                ThemeFile {
                    path: f.display().to_string(),
                    bytes: bytes.len() as u64,
                    blake3: blake3::hash(&bytes).to_hex().to_string(),
                    draft: draft.is_some(),
                }
            })
            .collect();
        let mut roles: Vec<ThemeRole> = theme
            .iter()
            .flat_map(|t| {
                t.roles().map(|name| {
                    let (width, height) = t.authored_size(name).unwrap_or_default();
                    ThemeRole {
                        name: name.to_owned(),
                        width,
                        height,
                        live: t.has_live(name),
                    }
                })
            })
            .collect();
        roles.sort_by(|a, b| a.name.cmp(&b.name));
        ThemeInfo {
            loaded: theme.is_some(),
            version: self.version(),
            paths: source
                .paths
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            mode: source.mode.clone(),
            modes: theme
                .as_ref()
                .map(|t| t.modes().to_vec())
                .unwrap_or_default(),
            roles,
            tokens: theme
                .iter()
                .flat_map(|t| {
                    t.tokens().map(|(name, value)| ThemeToken {
                        name: name.to_owned(),
                        value: value.to_string(),
                    })
                })
                .collect(),
            files,
            errors: source.errors.clone(),
        }
    }
}

/// Whether `a` and `b` name the same file, as given or as resolved.
fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// Whether `path` is a frame document: `.frame`, or the older `.fm`.
fn is_theme_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == "frame" || e == "fm")
}

/// The files `paths` come to, in layering order — a folder's by name —
/// each with when it last changed.
fn stamp(paths: &[PathBuf]) -> Stamp {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut inside: Vec<PathBuf> = std::fs::read_dir(path)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| is_theme_file(p))
                .collect();
            inside.sort();
            files.extend(inside);
        } else {
            files.push(path.clone());
        }
    }
    files
        .into_iter()
        .map(|f| {
            let modified = std::fs::metadata(&f).and_then(|m| m.modified()).ok();
            (f, modified)
        })
        .collect()
}

/// Compile `source`'s files, each its draft when it has one (`edited`'s
/// first), listing every problem: every file that cannot be read, every
/// one that does not parse, or everything the compile found.
fn compile(
    source: &Source,
    schema: &Schema,
    edited: Option<&BTreeMap<PathBuf, String>>,
) -> Result<Theme, Vec<ThemeError>> {
    if source.stamp.is_empty() {
        return Err(vec![ThemeError::new(
            ErrorKind::NoFiles,
            None,
            "no theme files",
        )]);
    }
    let mut scenes = Vec::new();
    let mut errors = Vec::new();
    for (file, _) in &source.stamp {
        let draft = edited
            .and_then(|e| e.get(file).map(String::as_str))
            .or_else(|| source.drafts.get(file).map(|d| d.markup.as_str()));
        let text = match draft {
            Some(d) => d.to_owned(),
            None => match std::fs::read_to_string(file) {
                Ok(t) => t,
                Err(e) => {
                    errors.push(ThemeError::new(ErrorKind::Io, Some(file), e.to_string()));
                    continue;
                }
            },
        };
        match crate::document::text::parse(&text, BTreeMap::new()) {
            Ok(scene) => scenes.push(scene),
            Err(e) => errors.push(ThemeError {
                line: u32::try_from(e.line).ok(),
                col: u32::try_from(e.col).ok(),
                ..ThemeError::new(ErrorKind::Parse, Some(file), e.message)
            }),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Theme::compile(
        Arc::new(crate::theme::runtime::layer(&scenes)),
        schema.clone(),
        source.mode.as_deref(),
    )
    .map_err(|ds| ds.into_iter().map(ThemeError::compile).collect())
}

/// One role drawn by [`ThemeHost::capture`].
#[cfg(feature = "embed")]
#[derive(Debug, Clone)]
pub struct Capture {
    pub pixels: crate::embed::cpu::Pixels,
    /// The host slots (`%name` nodes) shown, which the host paints and a
    /// capture leaves empty.
    pub slots: Vec<String>,
}

impl ThemeHost {
    /// The schema's values with `set`'s filled in, every other one
    /// missing: one instance's [`Fields`](crate::theme::runtime::Fields).
    #[must_use]
    pub fn fields(
        &self,
        set: &[(&str, crate::theme::runtime::Value)],
    ) -> Vec<crate::theme::runtime::Value> {
        let mut values = vec![crate::theme::runtime::Value::Missing; self.schema.names().count()];
        for (name, v) in set {
            if let Some(at) = self
                .schema
                .slot(name)
                .and_then(|s| usize::try_from(s).ok())
                .and_then(|s| values.get_mut(s))
            {
                *at = v.clone();
            }
        }
        values
    }

    /// Draw one instance of `role` from the current theme — both passes,
    /// at `size` (logical px; the role's own size when `None`) × `scale` —
    /// with Vello's CPU rasteriser, over transparency.
    ///
    /// # Errors
    /// When there is no theme, or it does not draw `role`.
    #[cfg(feature = "embed")]
    pub fn capture(
        &self,
        role: &str,
        size: Option<(f64, f64)>,
        scale: f64,
        fields: &dyn crate::theme::runtime::Fields,
    ) -> Result<Capture, ThemeError> {
        let theme = self
            .current()
            .ok_or_else(|| ThemeError::new(ErrorKind::Request, None, "no theme is loaded"))?;
        let Some(size) = size.or_else(|| theme.authored_size(role)) else {
            return Err(ThemeError::new(
                ErrorKind::Request,
                None,
                format!(
                    "the theme draws no `{role}` (roles: {})",
                    theme.roles().collect::<Vec<_>>().join(", ")
                ),
            ));
        };
        let scale = if scale > 0.0 { scale } else { 1.0 };
        let pixels = crate::embed::cpu::Rasterizer::new().render(
            (size.0 * scale).round().max(1.0) as u32,
            (size.1 * scale).round().max(1.0) as u32,
            scale,
            peniko::Color::TRANSPARENT,
            |canvas, at| {
                theme.record(role, size, fields, canvas, at);
                theme.live(role, size, fields, canvas, at);
            },
        );
        let slots = theme
            .slots(role, size, fields)
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        Ok(Capture { pixels, slots })
    }
}
