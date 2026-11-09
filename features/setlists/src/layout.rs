//! Where libraries, songs and setlists are, as vault paths.
//!
//! ```text
//! Libraries/<Library>/<Library>.md            a song library: a collection (type: song-library)
//! Libraries/<Library>/<Song>/<Song>.md        a song (type: song; categories, arrangements)
//!   <Song>.kf, <Song>.chart-<n>.png, <Song> - Master Rhythm.pdf, <Song>.lrc, the session
//!   Media/…                                   everything else of the song's
//!   Arrangements/<Arr>/<Song> (<Arr>).kf …    an arrangement: its own chart, pages, PDF
//! Setlists/<Set>.md                           a setlist (type: setlist), songs from any library
//! ```
//!
//! A library is a collection of songs kept with everything of theirs,
//! whether or not anyone is working on them; its songs are grouped by
//! category (a song's `categories`, several allowed). A setlist is not in
//! a library: it takes songs from any. Vault paths use `/` everywhere and
//! have no leading slash.

/// Where libraries go.
pub const LIBRARIES: &str = "Libraries";
/// Where setlists go.
pub const SETLISTS: &str = "Setlists";
/// The folder of a song's other files.
pub const MEDIA: &str = "Media";
/// The folder of a song's arrangements.
pub const ARRANGEMENTS: &str = "Arrangements";

/// A song library: a folder of `Libraries/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    pub name: String,
    /// Its folder (`Libraries/Christmas`).
    pub folder: String,
}

impl Library {
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            folder: format!("{LIBRARIES}/{name}"),
        }
    }

    /// Its own note, the folder's note (`Libraries/X/X.md`).
    #[must_use]
    pub fn note(&self) -> String {
        format!("{}/{}.md", self.folder, self.name)
    }

    /// The folder of song `title`.
    #[must_use]
    pub fn song_dir(&self, title: &str) -> String {
        format!("{}/{title}", self.folder)
    }

    /// The note of song `title`.
    #[must_use]
    pub fn song_note(&self, title: &str) -> String {
        format!("{}/{title}.md", self.song_dir(title))
    }

    /// A file named `name` in song `title`'s folder.
    #[must_use]
    pub fn song_file(&self, title: &str, name: &str) -> String {
        format!("{}/{name}", self.song_dir(title))
    }

    /// The folder of song `title`'s arrangement `arrangement`.
    #[must_use]
    pub fn arrangement_dir(&self, title: &str, arrangement: &str) -> String {
        format!("{}/{ARRANGEMENTS}/{arrangement}", self.song_dir(title))
    }

    /// A song's note path without `.md`: what a wikilink to it names.
    #[must_use]
    pub fn song_link_target(&self, title: &str) -> String {
        format!("{}/{title}", self.song_dir(title))
    }
}

/// The note of setlist `title`.
#[must_use]
pub fn setlist_note(title: &str) -> String {
    format!("{SETLISTS}/{title}.md")
}

/// A song as a setlist names it: which library's, and which arrangement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SongRef {
    pub library: String,
    pub title: String,
    /// One of its arrangements; None: the song's own chart.
    pub arrangement: Option<String>,
}

impl SongRef {
    #[must_use]
    pub fn new(library: &str, title: &str, arrangement: Option<&str>) -> Self {
        Self {
            library: library.to_owned(),
            title: title.to_owned(),
            arrangement: arrangement.map(str::to_owned),
        }
    }

    #[must_use]
    pub fn library(&self) -> Library {
        Library::named(&self.library)
    }

    /// `Song`, or `Song (Acoustic)`.
    #[must_use]
    pub fn label(&self) -> String {
        self.arrangement
            .as_ref()
            .map_or_else(|| self.title.clone(), |a| format!("{} ({a})", self.title))
    }

    /// What a wikilink to it names: the song's note, and the arrangement's
    /// heading on it (`…/Song#Acoustic`).
    #[must_use]
    pub fn link_target(&self) -> String {
        let note = self.library().song_link_target(&self.title);
        self.arrangement
            .as_ref()
            .map_or_else(|| note.clone(), |a| format!("{note}#{a}"))
    }

    /// The song a link target names (`Libraries/<L>/<S>/<S>[#<A>]`).
    #[must_use]
    pub fn from_link_target(target: &str) -> Option<Self> {
        let (path, arrangement) = match target.split_once('#') {
            Some((p, a)) => (p, Some(a.trim()).filter(|a| !a.is_empty())),
            None => (target, None),
        };
        let path = path.trim();
        let path = path.strip_suffix(".md").unwrap_or(path);
        let rest = path.strip_prefix(&format!("{LIBRARIES}/"))?;
        let mut parts = rest.split('/');
        let (library, dir, file) = (parts.next()?, parts.next()?, parts.next()?);
        (parts.next().is_none() && dir == file).then(|| Self::new(library, dir, arrangement))
    }

    /// Read `[Library/]Song[#Arrangement]`, the library given when the
    /// spec names none.
    #[must_use]
    pub fn parse(spec: &str, library: Option<&str>) -> Option<Self> {
        let (song, arrangement) = match spec.split_once('#') {
            Some((s, a)) => (s, Some(a.trim()).filter(|a| !a.is_empty())),
            None => (spec, None),
        };
        let (lib, title) = match song.split_once('/') {
            Some((l, t)) => (l.trim(), t.trim()),
            None => (library?, song.trim()),
        };
        (!lib.is_empty() && !title.is_empty()).then(|| Self::new(lib, title, arrangement))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        let lib = Library::named("Christmas");
        assert_eq!(lib.note(), "Libraries/Christmas/Christmas.md");
        assert_eq!(
            lib.song_note("Silent Night"),
            "Libraries/Christmas/Silent Night/Silent Night.md"
        );
        assert_eq!(
            lib.arrangement_dir("Silent Night", "Acoustic"),
            "Libraries/Christmas/Silent Night/Arrangements/Acoustic"
        );
        assert_eq!(setlist_note("Christmas Eve"), "Setlists/Christmas Eve.md");
    }

    #[test]
    fn song_refs_read_back() {
        let r = SongRef::new("Christmas", "Silent Night", Some("Acoustic"));
        assert_eq!(r.label(), "Silent Night (Acoustic)");
        assert_eq!(
            r.link_target(),
            "Libraries/Christmas/Silent Night/Silent Night#Acoustic"
        );
        assert_eq!(SongRef::from_link_target(&r.link_target()), Some(r.clone()));
        assert_eq!(
            SongRef::parse("Christmas/Silent Night#Acoustic", None),
            Some(r)
        );
        assert_eq!(
            SongRef::parse("Silent Night", Some("Christmas")),
            Some(SongRef::new("Christmas", "Silent Night", None))
        );
        assert_eq!(SongRef::parse("Silent Night", None), None);
        assert_eq!(SongRef::from_link_target("Projects/X/Y/Y"), None);
    }
}
