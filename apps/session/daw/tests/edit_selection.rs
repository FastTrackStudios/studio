//! Editing what is selected, end to end on the golden session: the kick
//! and the snare marked where the drummer played them, then the selection
//! — given in the call as a model gives it, or published as the
//! arrangement publishes its own — quantized and leveled through the
//! Action List (`EDIT_QUANTIZE`, `EDIT_LEVEL`, `EDIT_SCOPE`). The kit is
//! one unit: whichever of its tracks are selected, QUANTIZE's moves over
//! the same stretch; leveling, only the selected pieces. A track with no
//! editing profile is refused.
//!
//! One test, one process: the daw facade is bound to the first engine
//! opened.

use std::path::Path;

use dynamic_template::golden_session::{perform, shape};
use editing::scope::{Scope, Span};
use session_daw::actions::list;

/// Run `id` with `args`, as a model does.
fn run(id: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    list()
        .run_json(id, args.to_string().as_bytes())
        .map(|said| serde_json::from_slice(&said).unwrap_or(serde_json::Value::Null))
        .map_err(|e| e.to_string())
}

/// Every move a dry run lists, `(hit, shift ms)`, in order.
fn moves(spans: &serde_json::Value) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = spans
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|span| {
            span["result"]["biggest"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|m| {
            // To the microsecond: equal plans, equal numbers.
            let us = |v: f64| num_traits::ToPrimitive::to_i64(&(v * 1_000_000.0).round());
            Some((
                us(m["time"].as_f64()?)?,
                us(m["shift_ms"].as_f64()? / 1_000.0)?,
            ))
        })
        .collect();
    out.sort_unstable();
    out
}

/// r[verify quantize.selection]
#[test]
fn the_selection_is_what_an_edit_acts_on() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../features/dynamic-template/fixtures/golden/template.rpp");
    dynamic_template::golden_session::write_media(path.parent().expect("the fixtures"))
        .expect("the golden media writes");
    session_daw::open::open_silent(&path).expect("the golden session opens");

    let song = shape::maximal();
    let bar = 240.0 / song.bpm;
    let section = song
        .sections
        .iter()
        .find(|s| s.start >= 8 && s.end > s.start.saturating_add(3))
        .expect("a section past the intro");
    let (from, to) = (f64::from(section.start) * bar, f64::from(section.end) * bar);
    for (on, piece, _) in perform::groove(&song) {
        let name = match piece {
            perform::Piece::Kick => "Kick",
            perform::Piece::Snare => "Snare",
            _ => continue,
        };
        if on < from - 2.0 * bar || on > to + 2.0 * bar {
            continue;
        }
        let played = perform::kit_played(on, piece, song.bpm);
        run(
            "HITS_ADD",
            &serde_json::json!({ "piece": name, "time": played, "snap": false }),
        )
        .expect("a hit placed");
    }

    // Nothing selected, nothing said: refused, not the whole song.
    let refused = run("EDIT_QUANTIZE", &serde_json::json!({ "dry_run": true }))
        .expect_err("nothing selected");
    assert!(refused.contains("nothing selected"), "{refused}");

    // The kit's folder, or a piece's, is the drums.
    let scope = run(
        "EDIT_SCOPE",
        &serde_json::json!({ "tracks": ["Drums/Snare"] }),
    )
    .expect("a scope");
    assert_eq!(scope["profile"], "drums", "{scope}");
    assert_eq!(scope["tracks"], serde_json::json!(["Snare"]), "{scope}");
    // A track with no profile is refused, by name.
    let refused = run("EDIT_SCOPE", &serde_json::json!({ "tracks": ["Guide"] }))
        .expect_err("no profile for the guide");
    assert!(refused.contains("Guide"), "{refused}");

    // Selecting a piece quantizes the whole kit — as QUANTIZE over the
    // same stretch does, move for move.
    let settings = serde_json::json!({ "grid": 16, "target": "Grid", "dry_run": true });
    let mut by_hand = settings.clone();
    by_hand["from"] = from.into();
    by_hand["to"] = to.into();
    let quantize = run("QUANTIZE", &by_hand).expect("QUANTIZE");
    let expected = moves(&quantize);
    assert!(
        !expected.is_empty(),
        "the drummer is off the grid: {quantize}"
    );
    let mut selected = by_hand.clone();
    selected["tracks"] = serde_json::json!(["Drums/Snare"]);
    let edit = run("EDIT_QUANTIZE", &selected).expect("EDIT_QUANTIZE");
    assert_eq!(edit["on"]["profile"], "drums");
    assert_eq!(moves(&edit["spans"]), expected, "{edit}");

    // The arrangement's selection, published, is used when the call names
    // none: the same edit.
    let (daw, project) = session_daw::actions::song().expect("the song is open");
    let drums = daw::service::Tracks::all(daw, project)
        .into_iter()
        .find(|t| t.name == "Drums" && t.parent_guid.is_none())
        .expect("the kit's folder")
        .guid;
    session_daw::edit_actions::publish(Scope {
        tracks: vec![drums],
        spans: vec![Span::between(from, to)],
    });
    let published = run("EDIT_QUANTIZE", &settings).expect("over the selection");
    assert_eq!(moves(&published["spans"]), expected, "{published}");

    // Two stretches: a span each, quantized as one edit.
    let half = (from + to) / 2.0;
    let mut two = settings.clone();
    two["spans"] =
        serde_json::json!([{ "from": from, "to": half }, { "from": half + bar, "to": to }]);
    let split = run("EDIT_QUANTIZE", &two).expect("two stretches");
    assert_eq!(split["spans"].as_array().map(Vec::len), Some(2), "{split}");

    // Leveling takes only the pieces selected, and over a stretch only
    // the strokes in it.
    let level = |args: serde_json::Value| run("EDIT_LEVEL", &args).expect("EDIT_LEVEL");
    let snare = level(serde_json::json!({ "tracks": ["Drums/Snare"], "dry_run": true }));
    let pieces: Vec<&str> = snare["pieces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["piece"].as_str())
        .collect();
    assert_eq!(pieces, ["Snare"], "{snare}");
    let strokes = |said: &serde_json::Value| said["pieces"][0]["strokes"].as_u64().unwrap_or(0);
    let stretch = level(serde_json::json!({
        "tracks": ["Drums/Snare"], "from": from, "to": half, "dry_run": true,
    }));
    assert!(
        strokes(&stretch) > 0 && strokes(&stretch) < strokes(&snare),
        "a stretch levels its own strokes: {stretch} of {snare}"
    );
    let refused = run(
        "EDIT_LEVEL",
        &serde_json::json!({ "tracks": ["Guide"], "dry_run": true }),
    )
    .expect_err("the guide is not leveled as drums");
    assert!(refused.contains("Guide"), "{refused}");
}
