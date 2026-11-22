//! A setlist file (`<Set>.setlist`): the set a folder of song sessions
//! plays, as the session tools write it. One song per line, a path to
//! the song's session folder relative to the file; `#` lines are comments.
//!
//! ```text
//! # JT's Senior Recital — split from show-v2.
//!
//! # Act I: “The Originals”
//! cinematic-intro
//! drive-to-la
//!
//! # Archive (not played)
//! # better-strangers
//! ```
//!
//! A comment directly above a song (no blank line between) names the act
//! the songs under it belong to; a commented-out song is not played and
//! names nothing.

/// A part of the set: its name (from the comment above it) and its songs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Act {
    pub name: Option<String>,
    /// Each song's session folder, relative to the file's folder.
    pub songs: Vec<String>,
}

/// The set a file plays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SetFile {
    pub acts: Vec<Act>,
}

impl SetFile {
    /// Every song's folder, in playing order.
    pub fn songs(&self) -> Vec<&str> {
        self.acts
            .iter()
            .flat_map(|a| a.songs.iter().map(String::as_str))
            .collect()
    }
}

/// Read a setlist file.
pub fn parse(text: &str) -> SetFile {
    let mut out = SetFile::default();
    // The comment just above, until a blank line or a song.
    let mut comment: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            comment = None;
            continue;
        }
        if let Some(c) = line.strip_prefix('#') {
            let c = c.trim();
            // A commented-out song names nothing.
            comment = (!c.is_empty() && !looks_like_path(c)).then(|| act_name(c));
            continue;
        }
        let song = line.trim_end_matches('/').to_string();
        match comment.take() {
            Some(name) => out.acts.push(Act {
                name: Some(name),
                songs: vec![song],
            }),
            None => match out.acts.last_mut() {
                Some(a) => a.songs.push(song),
                None => out.acts.push(Act {
                    name: None,
                    songs: vec![song],
                }),
            },
        }
    }
    out
}

/// Where a song's folder is: the file's folder joined with the line.
pub fn folder(file_path: &str, song: &str) -> String {
    let dir = file_path.rsplit_once('/').map_or("", |(d, _)| d);
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for p in song.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// The set's name: the file's, without `.setlist`.
pub fn name_of(file_name: &str) -> String {
    file_name
        .strip_suffix(".setlist")
        .unwrap_or(file_name)
        .to_string()
}

/// A song's title from its folder's session file name (`Know Me.session`),
/// else from the folder's name (`know-me` → `Know Me`).
pub fn title_of(folder: &str, files: &[String]) -> String {
    for ext in [".fts-session", ".session"] {
        if let Some(f) = files.iter().find_map(|f| f.strip_suffix(ext)) {
            return f.to_string();
        }
    }
    let base = folder.rsplit('/').next().unwrap_or(folder);
    base.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `Act I: “The Originals”` → `Act I: The Originals`.
fn act_name(c: &str) -> String {
    c.replace(['“', '”', '"'], "")
}

/// A commented-out song line: one word, a folder name or a path.
fn looks_like_path(c: &str) -> bool {
    !c.contains(' ') && (c.contains('/') || c.contains('-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JT: &str =
        "# JT's Senior Recital — split from show-v2 (meters and tempo steps as the set has them).
# One song per line, relative to this file. Acts are comments; Archive is
# kept but not played.

# Act I: “The Originals”
cinematic-intro
drive-to-la

# Act II: “Royal Blood”
sleeptalker

# Archive (not played)
# better-strangers
# berenstein
";

    #[test]
    fn acts_are_the_comments_above_songs() {
        let s = parse(JT);
        assert_eq!(s.acts.len(), 2);
        assert_eq!(s.acts[0].name.as_deref(), Some("Act I: The Originals"));
        assert_eq!(s.acts[0].songs, vec!["cinematic-intro", "drive-to-la"]);
        assert_eq!(s.acts[1].name.as_deref(), Some("Act II: Royal Blood"));
        assert_eq!(
            s.songs(),
            vec!["cinematic-intro", "drive-to-la", "sleeptalker"]
        );
    }

    #[test]
    fn songs_under_no_comment_and_nested_folders() {
        let s = parse(
            "songs/sleigh-ride\nsongs/silent-night/\n\n# Tree Lighting - Concert Set\nsongs/feliz-navidad\n# songs/go-tell-it\n",
        );
        assert_eq!(s.acts[0].name, None);
        assert_eq!(
            s.acts[0].songs,
            vec!["songs/sleigh-ride", "songs/silent-night"]
        );
        assert_eq!(
            s.acts[1].name.as_deref(),
            Some("Tree Lighting - Concert Set")
        );
        assert_eq!(s.songs().len(), 3);
        assert_eq!(
            folder(
                "Projects/Merry and Bright 2026/Merry and Bright 2026.setlist",
                "songs/sleigh-ride"
            ),
            "Projects/Merry and Bright 2026/songs/sleigh-ride"
        );
        assert_eq!(folder("JT's Senior Recital.setlist", "know-me"), "know-me");
        assert_eq!(folder("a/b/x.setlist", "../c"), "a/c");
    }

    #[test]
    fn titles_and_names() {
        assert_eq!(
            name_of("JT's Senior Recital.setlist"),
            "JT's Senior Recital"
        );
        let files = vec!["Media".to_string(), "Know Me.session".to_string()];
        assert_eq!(title_of("know-me", &files), "Know Me");
        assert_eq!(title_of("songs/sleigh-ride", &[]), "Sleigh Ride");
    }
}
