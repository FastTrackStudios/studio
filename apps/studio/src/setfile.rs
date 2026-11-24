//! A setlist file (`<Set>.setlist`): the songs a set plays, one a line.
//! A setlist names songs, never places: a song line is the song's id (the
//! UUID in its session file's header) and its title as a hint, so the
//! file works on every device and from an external drive, wherever the
//! song folders are (`crate::song_ids` finds them). `= Title` is a song
//! with no session yet. Older files name a session folder by path,
//! relative to the file (`../Songs/know-me`) or absolute; those still
//! read. `# id: <uuid>` is the set's own id; `# org: <slug>` the org it
//! belongs to (else the org Studio is in); other `#` lines are comments.
//!
//! ```text
//! # id: 8d1c0f8e-0c84-4c1b-9a51-6f0f3a8b2d10
//! # org: codywright
//! # JT's Senior Recital — split from show-v2.
//!
//! # Act I: “The Originals”
//! df5b7031-b10a-44db-ac67-45af7bdcb8d6  Cinematic Intro
//! 45b794d1-d6ba-4d66-ab2d-6be35df2f6bc  Drive To LA
//!
//! # Archive (not played)
//! # better-strangers
//! ```
//!
//! A comment directly above a song (no blank line between) names the act
//! the songs under it belong to; a commented-out song is not played and
//! names nothing.

// A browser has no disk: what reads and writes it is the native build's.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

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
    /// The set's own id (`# id: …`), when the file has one.
    pub id: Option<String>,
    /// The org the set belongs to (`# org: <slug>`), when the file says.
    pub org: Option<String>,
    /// What kind of set it is (`# kind: studio`): a live show by default.
    pub kind: Kind,
    pub acts: Vec<Act>,
}

/// What a set is for, which decides its session's windows (`crate::stage`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// A show played live: the lighting on the right display.
    #[default]
    Live,
    /// A studio project (`# kind: studio`), an album or a record being
    /// made: the mixer on the right display, no lighting.
    Studio,
}

impl Kind {
    fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "live" | "show" => Some(Self::Live),
            "studio" | "project" | "record" | "album" => Some(Self::Studio),
            _ => None,
        }
    }
}

static KINDS: std::sync::Mutex<Vec<(String, Kind)>> = std::sync::Mutex::new(Vec::new());

/// Notes the kind of the set named `name`, as its file says, for the
/// session that opens it.
pub fn note_kind(name: &str, kind: Kind) {
    let mut kinds = KINDS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    kinds.retain(|(n, _)| n != name);
    kinds.push((name.to_owned(), kind));
}

/// The kind of the set named `name`, as its file said when it was last
/// read (a live show when it was never read).
#[must_use]
pub fn kind_of(name: &str) -> Kind {
    KINDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|(n, _)| n == name)
        .map_or(Kind::Live, |(_, k)| *k)
}

/// What a song line names.
#[derive(Clone, Debug, PartialEq)]
pub enum Song<'a> {
    /// A session's id, and the title written beside it.
    Id { id: &'a str, hint: &'a str },
    /// A song with no session yet.
    Title(&'a str),
    /// A session folder by path (older files).
    Path(&'a str),
}

/// Read a song line.
pub fn song(line: &str) -> Song<'_> {
    if let Some(t) = title_line(line) {
        return Song::Title(t);
    }
    let (first, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    if is_uuid(first) {
        return Song::Id {
            id: first,
            hint: rest.trim(),
        };
    }
    Song::Path(line)
}

/// `8-4-4-4-12` hex digits.
pub fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
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
            if let Some(id) = c.strip_prefix("id:").map(str::trim).filter(|i| is_uuid(i)) {
                out.id = Some(id.to_string());
                continue;
            }
            if let Some(org) = c.strip_prefix("org:").map(str::trim).filter(|o| is_slug(o)) {
                out.org = Some(org.to_string());
                continue;
            }
            if let Some(kind) = c.strip_prefix("kind:").and_then(Kind::parse) {
                out.kind = kind;
                continue;
            }
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

/// An org's slug: lower-case letters, digits and `-`.
pub fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The org a setlist file's set belongs to: the one its `# org:` line
/// names, else `here` (the org Studio is in).
pub fn org_of(text: &str, here: &str) -> String {
    parse(text).org.unwrap_or_else(|| here.to_string())
}

/// A song line that names a song by its title alone (`= Amazing Grace`):
/// it has no session folder yet.
pub fn title_line(song: &str) -> Option<&str> {
    song.strip_prefix('=')
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

/// Where a song's folder is: an absolute line as it is, else the file's
/// folder joined with the line (keeping the file path's leading `/`).
pub fn folder(file_path: &str, song: &str) -> String {
    if song.starts_with('/') {
        return song.trim_end_matches('/').to_string();
    }
    let lead = if file_path.starts_with('/') { "/" } else { "" };
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
    format!("{lead}{}", parts.join("/"))
}

/// A song as a line of a file Studio writes.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    /// Its session's id, and its title as a hint.
    Id { id: String, title: String },
    /// A line kept as the file had it (an older path line).
    Kept(String),
    /// No session yet: its title.
    Title(String),
}

/// Write a set as a setlist file: its id, a comment (`head`), then each
/// act under its name as a comment, one song a line. `parse` reads it back.
pub fn write(id: &str, head: &str, acts: &[(Option<String>, Vec<Line>)]) -> String {
    let mut out = format!("# id: {id}\n");
    for l in head.lines() {
        out.push_str(&format!("# {l}\n"));
    }
    for (name, songs) in acts {
        out.push('\n');
        if let Some(n) = name {
            out.push_str(&format!("# {n}\n"));
        }
        for s in songs {
            match s {
                Line::Id { id, title } => out.push_str(&format!("{id}  {title}")),
                Line::Kept(l) => out.push_str(l),
                Line::Title(t) => out.push_str(&format!("= {t}")),
            }
            out.push('\n');
        }
    }
    out
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
    // `Songs/<Title - Artist>/`: the title is before the artist.
    if let Some((title, _artist)) = base.split_once(" - ") {
        return title.trim().to_string();
    }
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

/// A commented-out song line: one word, a folder name or a path, or an
/// id line.
fn looks_like_path(c: &str) -> bool {
    matches!(song(c), Song::Id { .. }) || (!c.contains(' ') && (c.contains('/') || c.contains('-')))
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
    fn absolute_lines_are_used_as_they_are() {
        assert_eq!(
            folder(
                "Setlists/JT.setlist",
                "/home/me/Drive/Studio/Songs/know-me/"
            ),
            "/home/me/Drive/Studio/Songs/know-me"
        );
        assert_eq!(
            folder("/home/me/Drive/Studio/Setlists/JT.setlist", "../Songs/x"),
            "/home/me/Drive/Studio/Songs/x"
        );
    }

    const SET: &str = "8d1c0f8e-0c84-4c1b-9a51-6f0f3a8b2d10";
    const DTLA: &str = "45b794d1-d6ba-4d66-ab2d-6be35df2f6bc";

    #[test]
    fn studio_writes_what_it_reads() {
        let text = write(
            SET,
            "Studio set s1",
            &[
                (
                    None,
                    vec![Line::Id {
                        id: DTLA.into(),
                        title: "Drive To LA".into(),
                    }],
                ),
                (
                    Some("Act II".into()),
                    vec![
                        Line::Title("Amazing Grace".into()),
                        Line::Kept("../b".into()),
                    ],
                ),
            ],
        );
        assert_eq!(
            text,
            format!(
                "# id: {SET}\n# Studio set s1\n\n{DTLA}  Drive To LA\n\n# Act II\n= Amazing Grace\n../b\n"
            )
        );
        let s = parse(&text);
        assert_eq!(s.id.as_deref(), Some(SET));
        assert_eq!(s.acts.len(), 2);
        assert_eq!(s.acts[0].name, None);
        assert_eq!(
            song(&s.acts[0].songs[0]),
            Song::Id {
                id: DTLA,
                hint: "Drive To LA"
            }
        );
        assert_eq!(s.acts[1].name.as_deref(), Some("Act II"));
        assert_eq!(song(&s.acts[1].songs[0]), Song::Title("Amazing Grace"));
        assert_eq!(song(&s.acts[1].songs[1]), Song::Path("../b"));
    }

    #[test]
    fn an_id_line_commented_out_names_no_act() {
        let s = parse(&format!("# {DTLA}  Drive To LA\nknow-me\n"));
        assert_eq!(s.acts[0].name, None);
        assert!(is_uuid(DTLA));
        assert!(!is_uuid("drive-to-la"));
    }

    #[test]
    fn a_set_names_its_org_else_it_is_the_org_studio_is_in() {
        let text = format!(
            "# id: {SET}\n# org: crossroads-church\n# Days to Praise\n\n# Act I\n{DTLA}  Drive To LA\n"
        );
        let s = parse(&text);
        assert_eq!(s.org.as_deref(), Some("crossroads-church"));
        assert_eq!(s.id.as_deref(), Some(SET));
        // The org line is not a comment: the act is still named by the
        // comment directly above its first song.
        assert_eq!(s.acts.len(), 1);
        assert_eq!(s.acts[0].name.as_deref(), Some("Act I"));
        assert_eq!(org_of(&text, "codywright"), "crossroads-church");
        assert_eq!(
            org_of(&format!("# id: {SET}\n{DTLA}  Drive To LA\n"), "codywright"),
            "codywright"
        );
        // Not a slug: a comment, and the set is the org Studio is in.
        let odd = "# org: Crossroads Church\n= Amazing Grace\n";
        assert_eq!(parse(odd).org, None);
        assert_eq!(org_of(odd, "codywright"), "codywright");
        assert!(is_slug("days-to-praise") && !is_slug("-x") && !is_slug("A"));
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
        assert_eq!(title_of("Songs/Drive To LA - JT", &[]), "Drive To LA");
        let merged = vec!["Drive To LA.session".to_string()];
        assert_eq!(title_of("Songs/Drive To LA - JT", &merged), "Drive To LA");
    }
}
