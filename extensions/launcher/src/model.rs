//! What the launcher shows, read from the vault's notes: setlists in
//! their sets, each song with what it is ready with.
//!
//! Pure: rows in (a vault query's answer), values out. Nothing here
//! touches the host or the disk.

use serde_json::{Map, Value, json};

/// A note as a vault query's row: its path and properties.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub path: String,
    pub values: Map<String, Value>,
}

impl Row {
    fn text(&self, key: &str) -> Option<String> {
        match self.values.get(key)? {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }

    fn number(&self, key: &str) -> Option<f64> {
        match self.values.get(key)? {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    fn list(&self, key: &str) -> Vec<String> {
        match self.values.get(key) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        }
    }
}

/// The rows of a query's answer (`{rows: {groups: [{rows}]}}`), a value
/// named `note.<key>` read as `<key>`.
#[must_use]
pub fn rows_of(out: &Value) -> Vec<Row> {
    let table = out.get("rows").unwrap_or(out);
    table
        .get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|g| g.get("rows").and_then(Value::as_array))
        .flatten()
        .map(|r| {
            let mut values = Map::new();
            if let Some(m) = r.get("values").and_then(Value::as_object) {
                for (k, v) in m {
                    if !v.is_null() {
                        values.insert(k.strip_prefix("note.").unwrap_or(k).to_owned(), v.clone());
                    }
                }
            }
            Row {
                path: r
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                values,
            }
        })
        .collect()
}

/// A wikilink (`[[target#part|label]]`) taken apart; plain text is its
/// own target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// What it points at, without `#part`.
    pub target: String,
    /// The part after `#` (a song's arrangement).
    pub part: Option<String>,
    /// What it is shown as.
    pub label: String,
}

impl Link {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let inner = text
            .trim()
            .strip_prefix("[[")
            .and_then(|t| t.strip_suffix("]]"))
            .unwrap_or_else(|| text.trim());
        let (at, label) = match inner.split_once('|') {
            Some((at, label)) => (at, Some(label)),
            None => (inner, None),
        };
        let (target, part) = match at.split_once('#') {
            Some((target, part)) => (target, Some(part.to_owned())),
            None => (at, None),
        };
        let name = target.rsplit('/').next().unwrap_or(target);
        Self {
            target: target.to_owned(),
            part,
            label: label.unwrap_or(name).to_owned(),
        }
    }

    /// The note it points at (`target.md`).
    #[must_use]
    pub fn note(&self) -> String {
        let is_note = std::path::Path::new(&self.target)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("md"));
        if is_note {
            self.target.clone()
        } else {
            format!("{}.md", self.target)
        }
    }
}

/// A song of a library, as its note has it.
// Four independent facts about its files, each read from its own property.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Song {
    /// Its note (`Libraries/<lib>/<Song>/<Song>.md`).
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub key: Option<String>,
    /// The key it ends in, when another.
    pub key_end: Option<String>,
    pub tempo: Option<f64>,
    pub meter: Option<String>,
    /// How long it plays, in seconds.
    pub length: Option<f64>,
    /// It has a chart (a `.kf`).
    pub chart: bool,
    /// Its chart's file, a vault path.
    pub chart_path: Option<String>,
    /// Its cover, a vault path.
    pub art: Option<String>,
    /// It has timed lyrics (an `.lrc`).
    pub lyrics: bool,
    /// It has a recording of its own (a render) to rehearse to.
    pub recording: bool,
    /// It has the original's recording (`reference/`).
    pub reference: bool,
    /// The session its note names (`session`), as written.
    pub session: Option<String>,
}

impl Song {
    #[must_use]
    pub fn of(row: &Row) -> Self {
        let media = row.list("media");
        let is_reference = |m: &String| Link::parse(m).target.contains("/reference/");
        let name = row
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&row.path)
            .trim_end_matches(".md");
        Self {
            path: row.path.clone(),
            title: row.text("title").unwrap_or_else(|| name.to_owned()),
            artist: row.text("artist"),
            key: row.text("key"),
            key_end: row.text("key_end"),
            tempo: row.number("tempo"),
            meter: row.text("meter"),
            length: row.number("length"),
            chart: row.text("chart").is_some(),
            chart_path: row.text("chart").map(|c| Link::parse(&c).target),
            art: row.text("art").map(|a| Link::parse(&a).target),
            lyrics: row.text("lyrics").is_some(),
            recording: media.iter().any(|m| !is_reference(m)),
            reference: media.iter().any(is_reference),
            session: row.text("session").map(|s| Link::parse(&s).target),
        }
    }

    /// Its key as played: `C`, or `C→Db` when it ends in another.
    #[must_use]
    pub fn keys(&self) -> Option<String> {
        let key = self.key.as_ref()?;
        Some(match &self.key_end {
            Some(end) if end != key => format!("{key}→{end}"),
            _ => key.clone(),
        })
    }
}

/// A place in a setlist: the song's note, and how the setlist calls it
/// (`Know Me (Extended)`).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub label: String,
    /// The song's note path.
    pub note: String,
    pub arrangement: Option<String>,
    /// The song, when the library has its note.
    pub song: Option<Song>,
    /// What this setlist says of it (the setlist's `notes`, by its label).
    pub remark: Option<String>,
    /// Who leads it: its vocal and its guitar (the setlist's `leads`, by
    /// its label: `leads: {<label>: {vocal: Jake, guitar: Sam}}`).
    pub vocal: Option<String>,
    pub guitar: Option<String>,
    /// The key the setlist plays it in, when not its own (`keys`, by its
    /// label).
    pub key: Option<String>,
}

/// A rig a setlist is played with (the setlist's `parts`): whose it is,
/// which instrument and profile, and the patch a song takes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Part {
    pub player: String,
    /// The instrument (`guitar`, `keys`).
    pub rig: String,
    pub profile: Option<String>,
    /// A song's label to its patch.
    pub patches: Vec<(String, String)>,
}

impl Part {
    fn of(value: &Value) -> Option<Self> {
        let text = |k: &str| value.get(k).and_then(Value::as_str).map(str::to_owned);
        Some(Self {
            player: text("player").unwrap_or_default(),
            rig: text("rig")?,
            profile: text("profile"),
            patches: value
                .get("patches")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .filter_map(|(song, patch)| Some((song.clone(), patch.as_str()?.to_owned())))
                .collect(),
        })
    }

    /// As the setlist's frontmatter keeps it.
    #[must_use]
    pub fn json(&self) -> Value {
        let patches: Map<String, Value> = self
            .patches
            .iter()
            .map(|(song, patch)| (song.clone(), Value::String(patch.clone())))
            .collect();
        json!({ "player": self.player, "rig": self.rig, "profile": self.profile, "patches": patches })
    }
}

/// A named run of a setlist (an act, a set); unnamed when the setlist has
/// none.
#[derive(Debug, Clone, PartialEq)]
pub struct Set {
    pub name: Option<String>,
    pub entries: Vec<Entry>,
}

/// A setlist of the vault.
#[derive(Debug, Clone, PartialEq)]
pub struct Setlist {
    /// Its note (`Setlists/<name>.md`).
    pub path: String,
    pub title: String,
    pub date: Option<String>,
    /// The project it belongs to: its `project` link's folder, else its
    /// own title (the org's project root is `Projects/<this>`).
    pub project: String,
    /// The session file its note names (`session`), as written.
    pub session: Option<String>,
    /// The layout Open set applies (its note's `layout`): the one it was
    /// opened with last.
    pub layout: Option<String>,
    /// The rigs it is played with (its note's `parts`).
    pub parts: Vec<Part>,
    pub sets: Vec<Set>,
    /// Where and when the band meets (its note's `room`, `call`,
    /// `starts`).
    pub room: Option<String>,
    pub call: Option<String>,
    pub starts: Option<String>,
}

impl Setlist {
    #[must_use]
    pub fn of(row: &Row, songs: &[Song]) -> Self {
        let notes = row.values.get("notes").and_then(Value::as_object);
        let leads = row.values.get("leads").and_then(Value::as_object);
        let lead = |label: &str, part: &str| {
            leads
                .and_then(|l| l.get(label))
                .and_then(|l| l.get(part))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let entry = |text: &str| {
            let link = Link::parse(text);
            let note = link.note();
            Entry {
                key: row
                    .values
                    .get("keys")
                    .and_then(|k| k.get(&link.label))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                vocal: lead(&link.label, "vocal"),
                guitar: lead(&link.label, "guitar"),
                song: songs.iter().find(|s| s.path == note).cloned(),
                remark: notes
                    .and_then(|n| n.get(&link.label))
                    .and_then(Value::as_str)
                    .filter(|t| !t.trim().is_empty())
                    .map(str::to_owned),
                label: link.label,
                arrangement: link.part,
                note,
            }
        };
        let name = row
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&row.path)
            .trim_end_matches(".md");
        let title = row.text("title").unwrap_or_else(|| name.to_owned());
        let mut sets: Vec<Set> = row
            .values
            .get("sets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|set| Set {
                name: set.get("name").and_then(Value::as_str).map(str::to_owned),
                entries: set
                    .get("songs")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(entry)
                    .collect(),
            })
            .collect();
        if sets.is_empty() {
            sets.push(Set {
                name: None,
                entries: row.list("songs").iter().map(|s| entry(s)).collect(),
            });
        }
        // `[[Projects/<name>/project|…]]`: the folder's name.
        let project = row
            .text("project")
            .map(|p| Link::parse(&p).target)
            .and_then(|t| {
                t.strip_prefix("Projects/")
                    .and_then(|rest| rest.split('/').next())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| title.clone());
        Self {
            path: row.path.clone(),
            title,
            date: row.text("date"),
            project,
            session: row.text("session").map(|s| Link::parse(&s).target),
            layout: row.text("layout"),
            parts: row
                .values
                .get("parts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Part::of)
                .collect(),
            sets,
            room: row.text("room"),
            call: row.text("call"),
            starts: row.text("starts"),
        }
    }

    /// Every place of it, in order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.sets.iter().flat_map(|s| s.entries.iter())
    }

    /// How long it plays, in seconds: the songs that say, and how many
    /// do not.
    #[must_use]
    pub fn length(&self) -> (f64, usize) {
        let lengths: Vec<Option<f64>> = self
            .entries()
            .map(|e| e.song.as_ref().and_then(|s| s.length))
            .collect();
        (
            lengths.iter().flatten().sum(),
            lengths.iter().filter(|l| l.is_none()).count(),
        )
    }
}

/// The vault's setlists (by title) over its songs.
#[must_use]
pub fn setlists(setlist_rows: &[Row], song_rows: &[Row]) -> Vec<Setlist> {
    let songs: Vec<Song> = song_rows.iter().map(Song::of).collect();
    let mut out: Vec<Setlist> = setlist_rows
        .iter()
        .map(|r| Setlist::of(r, &songs))
        .collect();
    out.sort_by_key(|a| a.title.to_lowercase());
    out
}

/// `3:16` for 196.5 seconds; `1:04:10` past the hour.
#[must_use]
pub fn clock(seconds: f64) -> String {
    // Whole seconds, never negative: a length in a note's frontmatter.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions
    )]
    let total = seconds.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// A title as a folder slug: `She's American` → `shes-american`.
#[must_use]
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if (c.is_whitespace() || c == '-' || c == '_')
            && !out.ends_with('-')
            && !out.is_empty()
        {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// One of the engine's recent lines (`name, ago — path (alternatives: …)`)
/// taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    pub name: String,
    pub when: String,
    pub path: String,
}

impl Recent {
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let (head, rest) = line.split_once(" — ")?;
        let path = rest
            .split_once(" (alternatives")
            .map_or(rest, |(p, _)| p)
            .trim();
        let (name, when) = head.rsplit_once(", ").unwrap_or((head, ""));
        (!path.is_empty()).then(|| Self {
            name: name.trim().to_owned(),
            when: when.trim().to_owned(),
            path: path.to_owned(),
        })
    }
}

/// The engine's `SESSION_RECENT` answer as lines: a list, or one text of
/// lines (or of a JSON list).
#[must_use]
pub fn recents(answer: &Value) -> Vec<Recent> {
    let lines: Vec<String> = match answer {
        Value::Array(a) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        Value::String(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            _ => text.lines().map(str::to_owned).collect(),
        },
        _ => Vec::new(),
    };
    lines.iter().filter_map(|l| Recent::parse(l)).collect()
}

/// A song as `STUDIO_LAUNCHER_LIST` says it: `session` the file found for
/// it, `lights` whether the project has lighting cues for it.
#[must_use]
pub fn entry_json(entry: &Entry, session: Option<&str>, lights: bool) -> Value {
    let song = entry.song.as_ref();
    json!({
        "song": entry.label,
        "note": entry.note,
        "arrangement": entry.arrangement,
        "known": song.is_some(),
        "artist": song.and_then(|s| s.artist.clone()),
        "key": song.and_then(Song::keys),
        "tempo": song.and_then(|s| s.tempo),
        "meter": song.and_then(|s| s.meter.clone()),
        "length": song.and_then(|s| s.length),
        "note": entry.remark,
        "ready": {
            "chart": song.is_some_and(|s| s.chart),
            "lyrics": song.is_some_and(|s| s.lyrics),
            "recording": song.is_some_and(|s| s.recording),
            "reference": song.is_some_and(|s| s.reference),
            "tracks": session.is_some(),
            "lights": lights,
        },
        "session": session,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, values: &Value) -> Row {
        Row {
            path: path.to_owned(),
            values: values.as_object().cloned().unwrap_or_default(),
        }
    }

    fn drive() -> Row {
        row(
            "Libraries/Recital/Drive To LA/Drive To LA.md",
            &json!({
                "title": "Drive To LA", "artist": "Nights Out", "key": "C", "key_end": "Db",
                "tempo": 160.0, "meter": "4/4", "length": 196.5,
                "chart": "[[Libraries/Recital/Drive To LA/Drive To LA.kf|Drive To LA.kf]]",
                "lyrics": "[[Libraries/Recital/Drive To LA/Drive To LA.lrc|Drive To LA.lrc]]",
                "media": [
                    "[[Libraries/Recital/Drive To LA/renders/drive-to-la.ogg|drive-to-la.ogg]]",
                    "[[Libraries/Recital/Drive To LA/reference/drive-to-la (original).ogg|x]]",
                ],
            }),
        )
    }

    #[test]
    fn a_link_is_its_target_part_and_label() {
        let link = Link::parse("[[Libraries/R/Know Me/Know Me#Extended|Know Me (Extended)]]");
        assert_eq!(link.target, "Libraries/R/Know Me/Know Me");
        assert_eq!(link.part.as_deref(), Some("Extended"));
        assert_eq!(link.label, "Know Me (Extended)");
        assert_eq!(link.note(), "Libraries/R/Know Me/Know Me.md");
        assert_eq!(Link::parse("[[Setlists/Show]]").label, "Show");
        assert_eq!(Link::parse("songs/a/A.session").target, "songs/a/A.session");
    }

    #[test]
    fn a_song_says_what_it_is_ready_with() {
        let song = Song::of(&drive());
        assert_eq!(song.keys().as_deref(), Some("C→Db"));
        assert!(song.chart && song.lyrics && song.recording && song.reference);
        assert_eq!(song.session, None);
        let bare = Song::of(&row(
            "Libraries/X/Jingle Bell Rock/Jingle Bell Rock.md",
            &json!({}),
        ));
        assert_eq!(bare.title, "Jingle Bell Rock");
        assert!(!bare.chart && !bare.lyrics && !bare.recording && !bare.reference);
    }

    #[test]
    fn a_setlist_keeps_its_sets_and_finds_its_songs() {
        let set = row(
            "Setlists/Recital.md",
            &json!({
                "title": "JT's Senior Recital",
                "sets": [
                    { "name": "Act I", "songs": ["[[Libraries/Recital/Drive To LA/Drive To LA|Drive To LA]]"] },
                    { "name": "Act II", "songs": ["[[Libraries/Recital/Gone/Gone|Gone]]"] },
                ],
                "songs": ["[[Libraries/Recital/Drive To LA/Drive To LA|Drive To LA]]"],
                "notes": { "Drive To LA": "in C, ends in Db", "Gone": " " },
                "layout": "Stage",
                "parts": [
                    { "player": "Cody", "rig": "guitar", "profile": "worship", "patches": { "Drive To LA": "clean" } },
                    { "player": "nobody" },
                ],
            }),
        );
        let lists = setlists(&[set], &[drive()]);
        let list = &lists[0];
        assert_eq!(list.project, "JT's Senior Recital");
        assert_eq!(list.sets.len(), 2);
        assert_eq!(
            list.sets[0].entries[0]
                .song
                .as_ref()
                .map(|s| s.title.as_str()),
            Some("Drive To LA")
        );
        assert!(list.sets[1].entries[0].song.is_none());
        assert_eq!(list.length(), (196.5, 1));
        assert_eq!(
            list.sets[0].entries[0].remark.as_deref(),
            Some("in C, ends in Db")
        );
        assert_eq!(list.sets[1].entries[0].remark, None);
        assert_eq!(list.layout.as_deref(), Some("Stage"));
        assert_eq!(list.parts.len(), 1);
        assert_eq!(list.parts[0].rig, "guitar");
        assert_eq!(
            list.parts[0].patches,
            [("Drive To LA".to_owned(), "clean".to_owned())]
        );
        assert_eq!(
            Part::of(&list.parts[0].json()).as_ref(),
            Some(&list.parts[0])
        );
    }

    #[test]
    fn a_setlist_without_sets_is_one_run_and_takes_its_project_link() {
        let set = row(
            "Setlists/Crescendum.md",
            &json!({
                "songs": ["[[Libraries/C/Unbreakable/Unbreakable|Unbreakable]]"],
                "project": "[[Projects/Crescendum Album/project|Crescendum]]",
            }),
        );
        let list = Setlist::of(&set, &[]);
        assert_eq!(list.title, "Crescendum");
        assert_eq!(list.project, "Crescendum Album");
        assert_eq!(list.sets.len(), 1);
        assert_eq!(list.sets[0].name, None);
    }

    #[test]
    fn rows_come_out_of_a_query_answer() {
        let out = json!({ "rows": { "groups": [ { "rows": [
            { "path": "Setlists/A.md", "values": { "note.title": "A", "note.date": null } },
        ] } ] } });
        let rows = rows_of(&out);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text("title").as_deref(), Some("A"));
        assert!(!rows[0].values.contains_key("date"));
    }

    #[test]
    fn times_and_slugs() {
        assert_eq!(clock(196.5), "3:17");
        assert_eq!(clock(3850.0), "1:04:10");
        assert_eq!(slug("She's American"), "shes-american");
        assert_eq!(slug("Heart Isn't Ready"), "heart-isnt-ready");
        assert_eq!(slug("Hey!"), "hey");
    }

    #[test]
    fn a_recent_line_is_its_name_when_and_path() {
        let line = "Drive To LA, 2 hours ago — /p/songs/drive-to-la/Drive To LA.session (alternatives: mix 2)";
        let recent = Recent::parse(line);
        assert_eq!(
            recent,
            Some(Recent {
                name: "Drive To LA".to_owned(),
                when: "2 hours ago".to_owned(),
                path: "/p/songs/drive-to-la/Drive To LA.session".to_owned(),
            })
        );
        assert_eq!(recents(&json!([line])).len(), 1);
        assert_eq!(recents(&Value::String(format!("{line}\nnothing"))).len(), 1);
    }
}
