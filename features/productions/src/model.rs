//! What a production is: its DAW (the project type its note has), its
//! stage, and the property keys its note is written with.

/// The DAW a production lives in: one project type each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Daw {
    Ableton,
    ProTools,
    Reaper,
    Logic,
}

impl Daw {
    pub const ALL: [Self; 4] = [Self::Ableton, Self::ProTools, Self::Reaper, Self::Logic];

    /// Its project type's id: what the note's `projectType` says.
    #[must_use]
    pub const fn project_type(self) -> &'static str {
        match self {
            Self::Ableton => "ableton-set",
            Self::ProTools => "protools-session",
            Self::Reaper => "reaper-session",
            Self::Logic => "logic-project",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ableton => "Ableton Live",
            Self::ProTools => "Pro Tools",
            Self::Reaper => "REAPER",
            Self::Logic => "Logic Pro",
        }
    }

    /// The DAW of project type `id`.
    #[must_use]
    pub fn of_project_type(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.project_type() == id)
    }
}

/// Where a production is on its way out the door.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Stage {
    #[default]
    Writing,
    Tracking,
    Editing,
    Mixing,
    Mastering,
    Delivered,
}

impl Stage {
    pub const ALL: [Self; 6] = [
        Self::Writing,
        Self::Tracking,
        Self::Editing,
        Self::Mixing,
        Self::Mastering,
        Self::Delivered,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Writing => "writing",
            Self::Tracking => "tracking",
            Self::Editing => "editing",
            Self::Mixing => "mixing",
            Self::Mastering => "mastering",
            Self::Delivered => "delivered",
        }
    }

    /// The stage `text` names, any case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        Self::ALL
            .into_iter()
            .find(|s| s.as_str().eq_ignore_ascii_case(text))
    }
}

/// The folder production notes go in: Architect's projects folder, so
/// its Projects extension lists them with every other project.
pub const FOLDER: &str = "Projects";
/// What a project's note is called inside its folder (the `project`
/// kind's).
pub const PROJECT_PAGE: &str = "project.md";
/// The folder the default bases are written to.
pub const BASES_FOLDER: &str = "Productions";

/// The property keys of a production's note.
pub mod keys {
    /// The `project` kind's own.
    pub const TYPE: &str = "type";
    pub const TITLE: &str = "title";
    pub const STATE: &str = "state";
    pub const DUE: &str = "due";
    pub const PROJECT_TYPE: &str = "projectType";

    /// A person's: written once with a default (`stage`) or never.
    pub const STAGE: &str = "stage";
    pub const CLIENT: &str = "client";
    /// A person's on an Ableton set: `true` lists its locators as songs
    /// even when nothing marks it as a show.
    pub const SHOW: &str = "show";

    /// Read from the file at every scan.
    pub const TEMPO: &str = "tempo";
    pub const KEY: &str = "key";
    pub const LENGTH: &str = "length";
    pub const TRACKS: &str = "tracks";
    pub const PLUGINS: &str = "plugins";
    pub const MISSING_MEDIA: &str = "missingMedia";
    pub const MISSING_MEDIA_COUNT: &str = "missingMediaCount";
    pub const LAST_SAVED: &str = "lastSaved";
    pub const CHANGED_THIS_WEEK: &str = "changedThisWeek";
    /// The project folder, or the lone file: what a re-scan knows it by.
    pub const LOCATION: &str = "location";
    /// The file its DAW opens.
    pub const FILE: &str = "file";
    /// A project folder's DAW files, relative to it.
    pub const FILES: &str = "files";
    pub const READ_ERROR: &str = "readError";
    /// An Ableton show set's setlist note.
    pub const SETLIST: &str = "setlist";
}

/// The `type` of a project note.
pub const PROJECT_KIND: &str = "project";

/// `name` as a folder and a wikilink target: the characters a path or a
/// link reads as structure become `-`.
#[must_use]
pub fn file_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .trim_start_matches('.')
        .chars()
        .map(|c| match c {
            '/' | '\\' | '[' | ']' | '|' | '#' | '^' | ':' => '-',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().to_owned();
    if cleaned.is_empty() {
        "Untitled".to_owned()
    } else {
        cleaned
    }
}

/// The note of the production in folder `name` (`Projects/<name>/project.md`).
#[must_use]
pub fn note_path(name: &str) -> String {
    format!("{FOLDER}/{name}/{PROJECT_PAGE}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_and_types_read_back() {
        assert_eq!(Stage::parse(" Mixing "), Some(Stage::Mixing));
        assert_eq!(Stage::parse("done"), None);
        assert_eq!(Stage::default().as_str(), "writing");
        for daw in Daw::ALL {
            assert_eq!(Daw::of_project_type(daw.project_type()), Some(daw));
        }
        assert_eq!(file_name(" Live: Set #2 "), "Live- Set -2");
        assert_eq!(note_path("Album"), "Projects/Album/project.md");
    }
}
