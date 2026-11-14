//! The song view's contract with the app (studio#491), in one place: what
//! it reads of a song note and of the playhead, and what it asks.
//!
//! A song note (`type: song`, `setlists::song`) is claimed by
//! [`VIEW`] with `reads = ["documents", "files"]` (architect#673): the app
//! hands it `note.body` (a share link's guest: as the link's build wrote
//! it, no path, no frontmatter) and `note.files` (the link's images as
//! `data:` URLs and `.lrc` as text, the note's own first).
//!
//! **Read from the note** ([`song_of_note`]):
//!
//! - the chart: the body's first ```` ```kf ```` fence
//!   ([`setlists::song::CHART_FENCE`]), engraved by the view to follow the
//!   playhead (`setlists::follow`);
//! - the recording: the body's embed of a file that plays (`![[…/joy.ogg]]`,
//!   which Studio writes beside the chart), the one the fence's `media=`
//!   names first: a vault path, what the playhead's `media` is;
//! - the timed lines: the `.lrc` the body embeds, among the handed files,
//!   else the body's ```` ```lrc ```` fence (a chart's own lyric lane, as
//!   Studio writes it for a song with no LRC).
//!
//! **Followed**: the playhead's state of this extension's source
//! ([`crate::queue::SOURCE`]) while its current item is this song (its
//! `media` is the song's recording, [`following`]); its time extrapolated
//! from `at` while it plays ([`position`]).
//!
//! **Asked** ([`seek`]): a bar or a line pressed plays the song from there:
//! `PLAYHEAD_SEEK {t}` when the song is current; `PLAYHEAD_ITEM {id, play}`
//! then the seek once it is ([`Pending`]) when the source queues it (a
//! setlist's view set it); else `PLAYHEAD_SET` with the song alone, at
//! `t`, playing. A song with no recording asks nothing: there is no clock
//! to move.

use serde_json::{Map, Value, json};

use crate::queue::{
    self, Ask, File, FileKind, PLAYHEAD_ITEM, PLAYHEAD_SET, SLOTS, SOURCE, art_in, files_of,
    folder_of, lrc_lines,
};

/// The view's id in the bundle and the manifest (`mount(view)`): a song
/// note's chart and lyrics.
pub const VIEW: &str = "song";

/// Moves the playhead within the current item.
pub const PLAYHEAD_SEEK: &str = "PLAYHEAD_SEEK";

/// The longest a [`Pending`] seek waits for its item, ms.
pub const PENDING_MS: f64 = 5_000.0;

/// A song as its note says it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Song {
    pub title: String,
    /// The chart's Keyflow source.
    pub chart: Option<String>,
    /// Its recording, a vault path the host takes as `media`.
    pub recording: Option<String>,
    /// Its timed lines, `(seconds, words)`, in time order.
    pub lines: Vec<(f64, String)>,
    /// Its cover among the handed files, by vault path.
    pub art: Option<String>,
}

/// Every fenced block of `body`: its info string and its text.
#[must_use]
pub fn fences(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut open: Option<(String, usize, String)> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        let ticks = trimmed.chars().take_while(|c| *c == '`').count();
        match &mut open {
            None if ticks >= 3 => {
                let info = trimmed.get(ticks..).unwrap_or_default().trim().to_owned();
                open = Some((info, ticks, String::new()));
            }
            None => {}
            Some((_, n, _))
                if ticks >= *n && trimmed.get(ticks..).is_some_and(|r| r.trim().is_empty()) =>
            {
                if let Some((info, _, text)) = open.take() {
                    out.push((info, text));
                }
            }
            Some((_, _, text)) => {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    out
}

/// The targets of `body`'s embeds (`![[target]]`, `![[target|alias]]`,
/// `![[target#heading]]`), in order.
#[must_use]
pub fn embeds(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find("![[") {
        let after = rest.get(at.saturating_add(3)..).unwrap_or_default();
        let Some(end) = after.find("]]") else { break };
        let inner = after.get(..end).unwrap_or_default();
        let target = inner
            .split(['|', '#'])
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if !target.is_empty() {
            out.push(target);
        }
        rest = after.get(end.saturating_add(2)..).unwrap_or_default();
    }
    out
}

/// The song read from the note the app handed; none without one.
#[must_use]
pub fn song_of_note(note: &Value) -> Option<Song> {
    if !note.is_object() {
        return None;
    }
    let body = note.get("body").and_then(Value::as_str).unwrap_or("");
    let title = note
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    let fenced = fences(body);
    let first_word = |info: &str| info.split_whitespace().next().unwrap_or("").to_owned();
    let chart_fence = fenced
        .iter()
        .find(|(info, _)| first_word(info) == setlists::song::CHART_FENCE);
    let hint = chart_fence.and_then(|(info, _)| {
        info.split_whitespace()
            .find_map(|w| w.strip_prefix("media="))
            .map(setlists::song::unescape_info)
    });
    let embedded = embeds(body);
    let playable: Vec<&String> = embedded
        .iter()
        .filter(|t| setlists::song::is_playable(t))
        .collect();
    let recording = hint
        .as_deref()
        .and_then(|h| {
            playable
                .iter()
                .find(|t| t.as_str() == h || t.ends_with(&format!("/{h}")))
        })
        .or_else(|| playable.first())
        .map(|t| (*t).clone())
        .filter(|t| queue::media_path(t));
    let files = files_of(note);
    let lines = lyrics(&files, &embedded, &fenced);
    // The song's folder: where the files it embeds are (its recording may
    // be in a `Media/` folder of its own).
    let folder = embedded
        .iter()
        .find(|t| t.to_ascii_lowercase().ends_with(".lrc"))
        .or(recording.as_ref())
        .map(|t| folder_of(t).trim_end_matches("Media/").to_owned())
        .unwrap_or_default();
    Some(Song {
        title,
        chart: chart_fence.map(|(_, text)| text.clone()),
        recording,
        lines,
        art: art_in(&files, &folder).filter(|a| queue::media_path(a)),
    })
}

/// The song's timed lines: the `.lrc` its body embeds, among the handed
/// files; else the body's `lrc` fence.
fn lyrics(files: &[File], embedded: &[String], fenced: &[(String, String)]) -> Vec<(f64, String)> {
    let handed = files
        .iter()
        .find(|f| f.kind == FileKind::Lyrics && embedded.iter().any(|e| e == &f.path));
    if let Some(f) = handed {
        return lrc_lines(&f.content);
    }
    fenced
        .iter()
        .find(|(info, _)| info.split_whitespace().next() == Some("lrc"))
        .map(|(_, text)| lrc_lines(text))
        .unwrap_or_default()
}

/// The item id this view gives the song when it queues it itself.
#[must_use]
pub fn song_id(recording: &str) -> String {
    format!("song|{recording}")
}

/// Whether the playhead's state in `props` is this extension's source.
fn own(props: &Value) -> bool {
    props.get("source").and_then(Value::as_str) == Some(SOURCE)
}

fn is_song(item: &Value, recording: &str) -> bool {
    item.get("media").and_then(Value::as_str) == Some(recording)
        || item.get("id").and_then(Value::as_str) == Some(song_id(recording).as_str())
}

/// Whether this song is the source's current item.
#[must_use]
pub fn following(props: &Value, song: &Song) -> bool {
    let Some(rec) = song.recording.as_deref() else {
        return false;
    };
    own(props) && props.get("item").is_some_and(|i| is_song(i, rec))
}

/// Where the source is at `now_ms` (ms since the epoch): its `time` at
/// `at`, moved on by `rate` while it plays.
#[must_use]
pub fn position(props: &Value, now_ms: f64) -> f64 {
    let n = |k: &str| {
        props
            .get(k)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
    };
    let time = n("time").unwrap_or(0.0).max(0.0);
    let playing = props
        .get("playing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let t = match (playing, n("at")) {
        (true, Some(at)) if now_ms > at => {
            let rate = n("rate").filter(|r| *r > 0.0).unwrap_or(1.0);
            ((now_ms - at) / 1000.0).mul_add(rate, time)
        }
        _ => time,
    };
    n("duration").filter(|d| *d > 0.0).map_or(t, |d| t.min(d))
}

/// The index of the last of `starts` (ascending) at or before `t`; none
/// before the first.
#[must_use]
pub fn at_or_before(starts: &[f64], t: f64) -> Option<usize> {
    starts.partition_point(|s| *s <= t).checked_sub(1)
}

/// A section as played, for the song's queue item: `{name, start, end,
/// color}`, `color` only as `#rrggbb` (what the host takes).
#[derive(Debug, Clone, PartialEq)]
pub struct Played {
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub color: Option<String>,
}

/// A CSS colour as `#rrggbb`: one already so, or `rgb(r, g, b)`.
#[must_use]
pub fn hex_color(css: &str) -> Option<String> {
    let c = css.trim();
    if c.len() == 7 && c.starts_with('#') && c.chars().skip(1).all(|h| h.is_ascii_hexdigit()) {
        return Some(c.to_ascii_lowercase());
    }
    let inner = c
        .strip_prefix("rgb(")
        .or_else(|| c.strip_prefix("rgba("))?
        .strip_suffix(')')?;
    let parts: Vec<u8> = inner
        .split([',', ' ', '/'])
        .filter(|p| !p.is_empty())
        .take(3)
        .map(|p| p.trim().parse::<u8>().ok())
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [r, g, b] => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        _ => None,
    }
}

/// The song alone as this source's queue: what [`seek`] sets when the
/// source does not queue it.
#[must_use]
pub fn source_of(
    song: &Song,
    sections: &[Played],
    tempo: Option<(f64, u8)>,
    t: f64,
) -> Option<Value> {
    let rec = song.recording.as_deref()?;
    let mut item = Map::new();
    item.insert("id".to_owned(), json!(song_id(rec)));
    let title = if song.title.is_empty() {
        "Song"
    } else {
        &song.title
    };
    item.insert(
        "title".to_owned(),
        json!(title.chars().take(200).collect::<String>()),
    );
    item.insert("media".to_owned(), json!(rec));
    if let Some(art) = &song.art {
        item.insert("art".to_owned(), json!(art));
    }
    let sections: Vec<Value> = sections
        .iter()
        .filter(|s| s.start.is_finite() && s.end >= s.start && s.start >= 0.0)
        .take(200)
        .map(|s| {
            let mut v = json!({"name": s.name.chars().take(40).collect::<String>(), "start": s.start, "end": s.end});
            if let (Some(c), Some(o)) = (&s.color, v.as_object_mut()) {
                o.insert("color".to_owned(), json!(c));
            }
            v
        })
        .collect();
    if !sections.is_empty() {
        item.insert("sections".to_owned(), Value::Array(sections));
    }
    if !song.lines.is_empty() {
        item.insert("marks".to_owned(), queue::marks(&song.lines));
    }
    if let Some((bpm, meter)) =
        tempo.filter(|(b, m)| (20.0..=400.0).contains(b) && (1..=16).contains(m))
    {
        item.insert(
            "tempo".to_owned(),
            json!({"bpm": bpm, "at": 0.0, "meter": meter}),
        );
    }
    let name: String = title.chars().take(64).collect();
    Some(json!({
        "name": name,
        "queue": [Value::Object(item)],
        "slots": SLOTS,
        "index": 0,
        "time": (t.max(0.0) * 1000.0).round() / 1000.0,
        "play": true,
    }))
}

/// A seek waiting for its item to be current: where, and since when (ms).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pending {
    pub t: f64,
    pub since: f64,
}

/// What pressing time `t` of the song asks, and a seek left waiting for
/// its item ([`due`]).
#[must_use]
pub fn seek(
    props: &Value,
    song: &Song,
    t: f64,
    now_ms: f64,
    set: impl FnOnce() -> Option<Value>,
) -> (Vec<Ask>, Option<Pending>) {
    let Some(rec) = song.recording.as_deref() else {
        return (Vec::new(), None);
    };
    let t = t.max(0.0);
    if following(props, song) {
        return (vec![(PLAYHEAD_SEEK, json!({ "t": t }))], None);
    }
    let queued = own(props)
        .then(|| props.get("queue").and_then(Value::as_array))
        .flatten()
        .and_then(|q| q.iter().find(|i| is_song(i, rec)))
        .and_then(|i| i.get("id").and_then(Value::as_str));
    if let Some(id) = queued {
        let asks = vec![(PLAYHEAD_ITEM, json!({"id": id, "play": true}))];
        let pending = (t > 0.0).then_some(Pending { t, since: now_ms });
        return (asks, pending);
    }
    set().map_or_else(
        || (Vec::new(), None),
        |args| (vec![(PLAYHEAD_SET, args)], None),
    )
}

/// What becomes of a waiting seek ([`due`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Due {
    /// Its item is current and its length known: ask it now.
    Now(Ask),
    /// Not yet.
    Wait,
    /// It waited [`PENDING_MS`]: the item never became current.
    Dropped,
}

/// The waiting seek, once its item is current and its length known.
#[must_use]
pub fn due(props: &Value, song: &Song, pending: Pending, now_ms: f64) -> Due {
    let known = props
        .get("duration")
        .and_then(Value::as_f64)
        .is_some_and(|d| d > 0.0);
    if following(props, song) && known {
        return Due::Now((PLAYHEAD_SEEK, json!({ "t": pending.t })));
    }
    if now_ms - pending.since > PENDING_MS {
        return Due::Dropped;
    }
    Due::Wait
}

#[cfg(test)]
mod tests;
