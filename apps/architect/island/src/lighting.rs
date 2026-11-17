//! The Lighting tab's view (`lighting`): the song playing, as its light
//! show — the prerendered `<Title> - Lights.mp4` beside its recording
//! (`ignition … render`, `just lighting render-recital`) — kept on the
//! playhead.
//!
//! An ordinary view of this extension: the same in the app and for a share
//! link's guest (a share link is the web app as its guest), reading only
//! what the host hands every view. This module is its contract, pure and
//! tested on the host; `lighting_view` draws it.
//!
//! **Props in:**
//! - the playhead's state of this extension's source (`architect.playhead`
//!   v0.1, as `queue` reads it): `source`, `item` (`{id, title, media, …}`,
//!   `media` the recording's vault path), `time`, `playing`;
//! - the video's URL: a file the host hands with the note, `{path, kind:
//!   "video", url}` (`note.files`), or `item.video` (`{path, url}`). **Neither
//!   exists yet**: Architect hands a view images and lyrics only, as bytes
//!   (`features/spaces/src/view.rs` `ViewFileKind`), and the view's frame
//!   allows no media (`features/sites/src/island.rs`, `media-src 'none'`).
//!   Until it does the view names the video it would play and says so.
//!
//! **Asks out:** only Review's ([`review_ask`]). The audio is the
//! playhead's; the video is muted and follows it ([`correction`]).
//!
//! **Review:** the view hands the song's note and its light-show file to
//! Architect's Review (`architect.review`, architect#809: timestamped
//! comments on media) — `services.call("architect.review", "open", {path,
//! file})`, sent as the host's service-call ask ([`review_ask`]). It draws
//! no comments of its own.

use serde_json::Value;

/// The view's id, in `extension.toml` and in a favourite's `target`.
pub const VIEW: &str = "lighting";

/// What a song's light-show video is called beside its recording.
pub const SUFFIX: &str = " - Lights.mp4";

/// How far the video may drift from the playhead before it is moved
/// (seconds): a frame and a half at 30 fps is invisible; a seek costs a
/// decode, so the video is left alone inside it.
pub const DRIFT: f64 = 0.25;

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_owned()
}

/// The light-show video of the song whose recording is at `media`: in the
/// song's folder (`<lib>/<Title>/`, the recording in its `Media/` or
/// beside the note), `Media/<Title> - Lights.mp4`.
///
/// `Libraries/JT Senior Recital/Fire/Media/fire.ogg` →
/// `Libraries/JT Senior Recital/Fire/Media/Fire - Lights.mp4`.
#[must_use]
pub fn video_path(media: &str) -> Option<String> {
    let media = media.trim().trim_matches('/');
    let (dir, file) = media.rsplit_once('/')?;
    if file.ends_with(SUFFIX) {
        return Some(media.to_owned());
    }
    let song = dir.strip_suffix("/Media").unwrap_or(dir);
    let title = song.rsplit('/').next().filter(|t| !t.is_empty())?;
    Some(format!("{song}/Media/{title}{SUFFIX}"))
}

/// The video the view plays, and where the host lets it be fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Video {
    /// Its vault path.
    pub path: String,
    /// What the frame may load; `None` while the host hands no media URL.
    pub url: Option<String>,
}

/// Whether the playhead's state in `props` is this extension's source.
fn own(props: &Value) -> bool {
    props.get("source").and_then(Value::as_str) == Some(crate::queue::SOURCE)
}

/// The video of the song playing, from the props: its path from the
/// item's recording, its URL from a handed `video` file of that path (or
/// the item's own `video`). None when the playhead is another source's or
/// on nothing.
#[must_use]
pub fn video_of(props: &Value, note: &Value) -> Option<Video> {
    if !own(props) {
        return None;
    }
    let item = props.get("item")?;
    if let Some(v) = item.get("video").filter(|v| v.is_object()) {
        let path = text(v, "path");
        let url = Some(text(v, "url")).filter(|u| !u.is_empty());
        if !path.is_empty() || url.is_some() {
            return Some(Video { path, url });
        }
    }
    let path = video_path(&text(item, "media"))?;
    let url = note
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|f| text(f, "kind") == "video" && text(f, "path") == path)
        .map(|f| text(f, "url"))
        .filter(|u| !u.is_empty());
    Some(Video { path, url })
}

/// Where the playhead is, for the video: `(time, playing)`.
#[must_use]
pub fn playhead(props: &Value) -> (f64, bool) {
    let time = props
        .get("time")
        .and_then(Value::as_f64)
        .filter(|t| t.is_finite() && *t >= 0.0)
        .unwrap_or(0.0);
    let playing = props
        .get("playing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (time, playing)
}

/// What the video element should do to show the playhead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Correction {
    /// Move to this time first (it drifted past [`DRIFT`], or it is paused
    /// somewhere else).
    pub seek: Option<f64>,
    /// Then play (`true`) or pause (`false`).
    pub play: bool,
}

/// The video at `video_time`, the playhead at `time`
/// (`playing` or not): what to do. Playing, the video is left to run
/// unless it has drifted; paused, it is put exactly on the playhead (a
/// paused frame is looked at).
#[must_use]
pub fn correction(video_time: f64, time: f64, playing: bool) -> Correction {
    let off = (video_time - time).abs();
    let seek = if playing {
        (off > DRIFT).then_some(time)
    } else {
        (off > 1e-3).then_some(time)
    };
    Correction {
        seek,
        play: playing,
    }
}

/// The song note a video belongs to: `<song folder>/<Title>.md`, the
/// folder the video's `Media/` is in.
#[must_use]
pub fn song_note(video: &str) -> Option<String> {
    let (dir, _) = video.trim_matches('/').rsplit_once('/')?;
    let song = dir.strip_suffix("/Media").unwrap_or(dir);
    let title = song.rsplit('/').next().filter(|t| !t.is_empty())?;
    Some(format!("{song}/{title}.md"))
}

/// The service the view hands a review to (`architect.review`,
/// architect#809), and its method.
pub const REVIEW_SERVICE: &str = "architect.review";
pub const REVIEW_OPEN: &str = "open";

/// What the view hands Review: the song's note and its light-show file
/// (`services.call("architect.review", "open", {path, file})`), and the
/// moment the view is on.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewTarget {
    /// The song note.
    pub path: String,
    /// The video, a file of that note.
    pub file: String,
    pub time: f64,
}

/// The review target of the view's video at the playhead; none for a
/// video whose note cannot be named.
#[must_use]
pub fn review_target(video: &Video, props: &Value) -> Option<ReviewTarget> {
    Some(ReviewTarget {
        path: song_note(&video.path)?,
        file: video.path.clone(),
        time: playhead(props).0,
    })
}

/// The ask that opens Review on the target: the host's service call
/// (`EXT_SERVICE_CALL {service, method, args}`, what a page's
/// `services.call` runs). A web view's frame forwards only the
/// playhead's asks today (architect `apps/web/src/app/frames.rs`,
/// `architect_playhead::ask`), so it is refused there until the frame
/// host passes service calls through.
#[must_use]
pub fn review_ask(target: &ReviewTarget) -> (String, Value) {
    (
        "EXT_SERVICE_CALL".to_owned(),
        serde_json::json!({
            "service": REVIEW_SERVICE,
            "method": REVIEW_OPEN,
            "args": { "path": target.path, "file": target.file },
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_video_sits_beside_the_recording_named_for_the_song() {
        assert_eq!(
            video_path("Libraries/JT Senior Recital/Fire/Media/fire.ogg").as_deref(),
            Some("Libraries/JT Senior Recital/Fire/Media/Fire - Lights.mp4")
        );
        assert_eq!(
            video_path("Libraries/JT Senior Recital/Hey!/Media/hey.ogg").as_deref(),
            Some("Libraries/JT Senior Recital/Hey!/Media/Hey! - Lights.mp4")
        );
        // A recording beside the note, not in Media/.
        assert_eq!(
            video_path("Libraries/L/Know Me/know-me.ogg").as_deref(),
            Some("Libraries/L/Know Me/Media/Know Me - Lights.mp4")
        );
        // Already the video.
        assert_eq!(
            video_path("Libraries/L/Fire/Media/Fire - Lights.mp4").as_deref(),
            Some("Libraries/L/Fire/Media/Fire - Lights.mp4")
        );
        assert_eq!(video_path("fire.ogg"), None);
    }

    fn props(media: &str) -> Value {
        json!({
            "source": crate::queue::SOURCE,
            "item": {"id": "s|Fire", "title": "Fire", "media": media},
            "time": 12.5, "playing": true,
        })
    }

    #[test]
    fn a_handed_video_file_gives_the_url_and_without_one_there_is_none() {
        let p = props("Libraries/L/Fire/Media/fire.ogg");
        let path = "Libraries/L/Fire/Media/Fire - Lights.mp4";
        let note = json!({"files": [
            {"path": "Libraries/L/Fire/cover.jpg", "kind": "image", "data": "data:image/jpeg;base64,"},
            {"path": path, "kind": "video", "url": "/s/abc/Libraries/L/Fire/Media/Fire%20-%20Lights.mp4"},
        ]});
        let v = video_of(&p, &note).unwrap();
        assert_eq!(v.path, path);
        assert_eq!(v.url.as_deref(), Some("/s/abc/Libraries/L/Fire/Media/Fire%20-%20Lights.mp4"));
        let bare = video_of(&p, &Value::Null).unwrap();
        assert_eq!((bare.path.as_str(), bare.url), (path, None));
    }

    #[test]
    fn another_sources_playhead_is_not_followed() {
        let mut p = props("Libraries/L/Fire/Media/fire.ogg");
        p["source"] = json!("app:share");
        assert_eq!(video_of(&p, &Value::Null), None);
    }

    #[test]
    fn playing_it_runs_free_inside_the_drift_and_is_moved_past_it() {
        assert_eq!(correction(10.1, 10.0, true), Correction { seek: None, play: true });
        assert_eq!(correction(10.4, 10.0, true), Correction { seek: Some(10.0), play: true });
        // A song changed: the video starts again from the playhead.
        assert_eq!(correction(187.0, 0.2, true).seek, Some(0.2));
    }

    #[test]
    fn paused_it_stands_exactly_on_the_playhead() {
        assert_eq!(correction(10.1, 10.0, false), Correction { seek: Some(10.0), play: false });
        assert_eq!(correction(10.0, 10.0, false).seek, None);
    }

    #[test]
    fn review_is_handed_the_song_note_and_its_lights_file() {
        let v = Video { path: "Libraries/L/Fire/Media/Fire - Lights.mp4".into(), url: None };
        let t = review_target(&v, &props("x")).unwrap();
        assert_eq!(t.path, "Libraries/L/Fire/Fire.md");
        assert_eq!(t.file, "Libraries/L/Fire/Media/Fire - Lights.mp4");
        assert_eq!(t.time, 12.5);
        let (action, args) = review_ask(&t);
        assert_eq!(action, "EXT_SERVICE_CALL");
        assert_eq!(
            args,
            json!({"service": "architect.review", "method": "open",
                   "args": {"path": "Libraries/L/Fire/Fire.md",
                            "file": "Libraries/L/Fire/Media/Fire - Lights.mp4"}})
        );
    }
}
