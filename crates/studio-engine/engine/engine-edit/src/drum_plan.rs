//! `DRUMS_PLAN`: where every playing drum hit should go, in one call.
//!
//! The engine has no planner of its own: it gathers what the plan needs —
//! the kit's playing hits (with their take, peak, what heard them and
//! their flags), the drum MIDI mapped to pieces, the bars, the sections
//! and the zones — into a `daw.edit` [`PlanRequest`], asks the chosen
//! provider (rhythm's `RHYTHM_PLAN`, through [`crate::algorithms::plan`])
//! and answers compactly: counts, the reasons, the lean per bar, the
//! doubts (bar.beat addressed), and on request the targets in
//! `DRUMS_APPLY`'s shape (`targets`, `remove`, `delete`, `rides`). With
//! `apply` it hands them to `DRUMS_APPLY` when the engine has it.
//!
//! The last plan is kept ([`last`]) so an apply can take it as it is.

#![cfg(feature = "native")]

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use architect::action::{ActionCaller, ActionList, ActionMeta, Effect, RawJson};
use daw::service::{ProjectContext, TempoMap as _};
use daw_edit::PlanRequest;
use daw_edit::plan::{
    Anchor, BarLine, Heard, Note, Plan, PlayedHit, Prefer, Reason, Section, Settings, Zone,
    ZoneKind,
};
use facet::Facet;
use serde_json::{Value as Json, json};

use crate::actions::{meta, song};

/// How far before a span's start a hit still belongs to it (a pickup
/// played early), seconds.
const LEAD_IN: f64 = 0.060;
/// The drum MIDI's notes are mapped to the piece whose hits land this
/// near them.
const MAP_WINDOW: f64 = 0.060;

static LAST: Mutex<Option<Plan>> = Mutex::new(None);

/// The last plan `DRUMS_PLAN` made, as the provider answered it.
#[must_use]
pub fn last() -> Option<Plan> {
    LAST.lock().ok().and_then(|l| l.clone())
}

static PLAN: ActionMeta = meta(
    "DRUMS_PLAN",
    "drums_plan",
    "Plan the drum edit",
    "Plans where every playing drum hit goes, in one call, by the chosen daw.edit provider (rhythm): the lean per half bar from the backbeat and the written notes, each drum's strokes to its written notes one-to-one and in order (else the nearest line after the lean), straight or triplet per beat, unisons/flams/graces/seam doubles as rides, double-kick runs on consecutive 16ths, false doubles removed. Over `sections` (names) or `from`/`to` (project seconds; the whole song when none). Settings: `grid_hint` ({section: 16|12|24}), `anchors` ([{piece, beats, section?}]), `lock_runs` (default true), `prefer` (midi|grid), `plan_fills` (plan fills with no grid of their own instead of leaving them), `map` ({piece: MIDI pitch}, instead of the learned map). Answers counts, `reasons`, `leans` ([bar, first half ms, second half ms]), `doubts` (only the hits worth a look: id, piece, `at` bar.beat, both candidate targets and why); `targets: true` adds `apply` ({targets: {id: seconds}, remove, delete, rides: {rider: lead}}, DRUMS_APPLY's shape); `detail: true` adds every hit (columns: id, target, reason, confidence). `apply: true` hands the plan to DRUMS_APPLY. `dump` writes the request (JSON) to that path, for a bench. Read-only unless `apply`.",
    "Drums",
    "",
);

#[derive(Facet, Default)]
struct PlanArg {
    #[facet(default)]
    sections: Vec<String>,
    from: Option<f64>,
    to: Option<f64>,
    #[facet(default)]
    grid_hint: HashMap<String, u32>,
    #[facet(default)]
    anchors: Vec<AnchorArg>,
    lock_runs: Option<bool>,
    prefer: Option<String>,
    plan_fills: Option<bool>,
    #[facet(default)]
    map: HashMap<String, u8>,
    apply: Option<bool>,
    targets: Option<bool>,
    detail: Option<bool>,
    dump: Option<String>,
}

#[derive(Facet, Default, Clone)]
struct AnchorArg {
    piece: String,
    #[facet(default)]
    beats: Vec<f64>,
    section: Option<String>,
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register_with(&PLAN, Effect::Mutating, |_: &ActionCaller, a: PlanArg| {
        let span = tracing::info_span!(
            "DRUMS_PLAN",
            plan.hits = tracing::field::Empty,
            plan.doubts = tracing::field::Empty,
            plan.apply = a.apply.unwrap_or(false),
        );
        let _in = span.enter();
        let (daw, project) = song()?;
        let started = web_time::Instant::now();
        let request = {
            let _s = tracing::info_span!("DRUMS_PLAN.build_request").entered();
            let r = build_request(daw, &project, &a)?;
            tracing::info!(
                plan.hits = r.hits.len(),
                plan.notes = r.notes.len(),
                plan.bars = r.bars.len(),
                plan.sections = r.sections.len(),
                plan.zones = r.zones.len(),
                "DRUMS_PLAN.build_request"
            );
            r
        };
        span.record("plan.hits", request.hits.len());
        let built = started.elapsed().as_secs_f64();
        if let Some(path) = &a.dump {
            let text = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
            std::fs::write(path, text).map_err(|e| format!("dump {path}: {e}"))?;
        }
        let answer = {
            let _s = tracing::info_span!("DRUMS_PLAN.provider", plan.hits = request.hits.len())
                .entered();
            let t = web_time::Instant::now();
            let p = crate::mix_calls::plan(&request)?;
            tracing::info!(
                plan.provider_ms = t.elapsed().as_secs_f64() * 1000.0,
                plan.targets = p.targets.len(),
                plan.rides = p.rides.len(),
                plan.remove = p.remove.len(),
                plan.delete = p.delete.len(),
                plan.doubts = p.doubts.len(),
                "DRUMS_PLAN.provider"
            );
            p
        };
        span.record("plan.doubts", answer.doubts.len());
        let planned = started.elapsed().as_secs_f64();
        let mut out = {
            let _s = tracing::info_span!("DRUMS_PLAN.answer").entered();
            let at = |t: f64| bar_beat(crate::song_bars::musical(daw, &project, t));
            compact(
                &answer,
                &request,
                &at,
                a.targets.unwrap_or(false),
                a.detail.unwrap_or(false),
            )
        };
        out["ms"] = json!({
            "build": round(built * 1000.0, 1),
            "provider": round((planned - built) * 1000.0, 1),
        });
        if let Ok(mut l) = LAST.lock() {
            *l = Some(answer.clone());
        }
        if a.apply.unwrap_or(false) {
            out["applied"] = apply(&answer);
        }
        Ok::<_, String>(RawJson::from(out.to_string()))
    })?;
    Ok(())
}

/// Hands `plan` to `DRUMS_APPLY` (the atomic apply), when the engine has
/// it: its answer, or why not.
fn apply(plan: &Plan) -> Json {
    let args = apply_shape(plan).to_string();
    match crate::actions::list().run_json("DRUMS_APPLY", args.as_bytes()) {
        Ok(out) => serde_json::from_slice(&out).unwrap_or(Json::Null),
        Err(e) => {
            tracing::warn!(error = %e, "DRUMS_PLAN: the plan was not applied");
            json!({ "error": format!("DRUMS_APPLY: {e}") })
        }
    }
}

/// The plan as `DRUMS_APPLY` takes it.
fn apply_shape(plan: &Plan) -> Json {
    json!({
        "targets": plan.targets,
        "remove": plan.remove,
        "delete": plan.delete,
        "rides": plan.rides,
    })
}

fn round(x: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (x * k).round() / k
}

/// `(bar, beat, fraction)` as the owner writes it: `118.4.25`.
fn bar_beat((bar, beat, frac): (i32, i32, f64)) -> String {
    let hundredths = (frac * 100.0).round() as i32;
    let (beat, hundredths) = if hundredths >= 100 {
        (beat + 1, 0)
    } else {
        (beat, hundredths)
    };
    format!("{bar}.{beat}.{hundredths:02}")
}

/// The request from the song: its playing hits in the span, the drum
/// MIDI mapped to pieces, the bars, sections and zones.
fn build_request(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    a: &PlanArg,
) -> Result<PlanRequest, String> {
    let sections: Vec<Section> = crate::moment::sections(daw, project)
        .into_iter()
        .map(|s| Section {
            name: s.name,
            from: s.start,
            to: s.end,
        })
        .collect();
    let span = span_of(&sections, &a.sections, a.from, a.to)?;
    let played: Vec<Hit> = crate::hits::playback(daw, project)
        .into_iter()
        .map(|p| Hit {
            id: p.id,
            piece: p.piece,
            time: p.time,
            peak_db: p.peak_db,
            from_trigger: p.mark.from_trigger,
            from_mic: p.mark.from_mic,
            ghost: p.mark.ghost,
            bleed: p.mark.bleed,
            flam: p.mark.flam_with.is_some(),
            unsure: p.mark.origin == daw_proto::MarkOrigin::Auto && p.mark.confidence < 0.6,
            false_double: crate::hit_flags::is_false_double(&p.mark.note),
        })
        .collect();
    let hits = played_in(&played, span);
    if hits.is_empty() {
        return Err("no playing drum hits in the span: detect them first (HITS_DETECT)".to_owned());
    }
    let notes = match crate::drum_reference::find_track(daw, project) {
        Some(track) => {
            let all = crate::drum_reference::notes(daw, project, &track);
            mapped_notes(&all, &hits, &a.map, span)
        }
        None => Vec::new(),
    };
    let bars = bars_over(daw, project, span);
    let zones = crate::hits::zones(daw, project)
        .into_iter()
        .filter(|z| z.origin != crate::hits::ZoneOrigin::Rejected)
        .filter(|z| z.end > span.0 && z.start < span.1)
        .map(|z| Zone {
            start: z.start,
            end: z.end,
            kind: match z.kind {
                crate::hits::ZoneKind::Fill => ZoneKind::Fill,
                crate::hits::ZoneKind::Exception => ZoneKind::Exception,
            },
            grid: z.grid,
        })
        .collect();
    Ok(PlanRequest {
        hits,
        notes,
        bars,
        sections,
        zones,
        settings: settings_of(a),
    })
}

/// A playing hit as the engine reads it.
#[derive(Clone, Debug)]
struct Hit {
    id: String,
    piece: String,
    time: f64,
    peak_db: f64,
    from_trigger: bool,
    from_mic: bool,
    ghost: bool,
    bleed: bool,
    flam: bool,
    unsure: bool,
    false_double: bool,
}

/// The span to plan: the named sections' (first to last), else
/// `from`/`to`, else everything.
fn span_of(
    sections: &[Section],
    names: &[String],
    from: Option<f64>,
    to: Option<f64>,
) -> Result<(f64, f64), String> {
    if !names.is_empty() {
        let chosen: Vec<&Section> = sections
            .iter()
            .filter(|s| names.iter().any(|n| n.eq_ignore_ascii_case(&s.name)))
            .collect();
        if chosen.len() != names.len() {
            let known: Vec<&str> = sections.iter().map(|s| s.name.as_str()).collect();
            return Err(format!("no such section among {known:?}"));
        }
        let lo = chosen.iter().map(|s| s.from).fold(f64::INFINITY, f64::min);
        let hi = chosen
            .iter()
            .map(|s| s.to)
            .fold(f64::NEG_INFINITY, f64::max);
        return Ok((lo, hi));
    }
    Ok((
        from.unwrap_or(f64::NEG_INFINITY),
        to.unwrap_or(f64::INFINITY),
    ))
}

/// The take a hit is from: its id's code after `~`.
fn take_of(id: &str) -> &str {
    id.rsplit_once(crate::hits::TAKE_TAG).map_or("", |(_, t)| t)
}

/// The hits that play in `span` (from a little before its start), as the
/// contract has them.
fn played_in(played: &[Hit], span: (f64, f64)) -> Vec<PlayedHit> {
    played
        .iter()
        .filter(|h| h.time >= span.0 - LEAD_IN && h.time < span.1)
        .map(|h| {
            let flags = [
                (h.ghost, "ghost"),
                (h.flam, "flam"),
                (h.bleed, "bleed"),
                (h.unsure, "unsure"),
                (h.false_double, crate::hit_flags::FALSE_DOUBLE),
            ]
            .into_iter()
            .filter(|(yes, _)| *yes)
            .map(|(_, f)| f.to_owned())
            .collect();
            PlayedHit {
                id: h.id.clone(),
                piece: h.piece.clone(),
                take: take_of(&h.id).to_owned(),
                time: h.time,
                peak_db: h.peak_db,
                heard: match (h.from_trigger, h.from_mic) {
                    (true, true) => Heard::Both,
                    (true, false) => Heard::Trigger,
                    (false, true) => Heard::Mic,
                    (false, false) => Heard::Unknown,
                },
                flags,
            }
        })
        .collect()
}

/// The drum MIDI's notes in `span`, each as its piece's: by `map` (piece
/// → pitch) where given, else learned (`drum_reference::learn_map`) with
/// one pitch per piece — the mapped pitch with the most notes (a cymbal
/// written on every kick maps to the kick too, but has fewer notes).
fn mapped_notes(
    notes: &[crate::drum_reference::RefNote],
    hits: &[PlayedHit],
    map: &HashMap<String, u8>,
    span: (f64, f64),
) -> Vec<Note> {
    let by_pitch: BTreeMap<u8, String> = if map.is_empty() {
        let mut times: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for h in hits {
            times.entry(h.piece.clone()).or_default().push(h.time);
        }
        let pieces: Vec<(String, Vec<f64>)> = times
            .into_iter()
            .map(|(p, mut t)| {
                t.sort_by(f64::total_cmp);
                (p, t)
            })
            .collect();
        let mut best: BTreeMap<String, (usize, u8)> = BTreeMap::new();
        for m in crate::drum_reference::learn_map(notes, &pieces, MAP_WINDOW) {
            if let Some(piece) = m.piece {
                let e = best.entry(piece).or_insert((0, m.pitch));
                if m.notes > e.0 {
                    *e = (m.notes, m.pitch);
                }
            }
        }
        best.into_iter()
            .map(|(piece, (_, pitch))| (pitch, piece))
            .collect()
    } else {
        map.iter()
            .map(|(piece, pitch)| (*pitch, piece.clone()))
            .collect()
    };
    notes
        .iter()
        .filter(|n| n.time >= span.0 - LEAD_IN - 0.1 && n.time < span.1 + 0.1)
        .filter_map(|n| {
            by_pitch.get(&n.pitch).map(|piece| Note {
                time: n.time,
                piece: piece.clone(),
                velocity: n.velocity,
            })
        })
        .collect()
}

/// Every bar from the one before `span` to the one after it, through the
/// tempo map.
fn bars_over(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    span: (f64, f64),
) -> Vec<BarLine> {
    let end = if span.1.is_finite() {
        span.1
    } else {
        // The song's end: the last section's, else ten minutes.
        crate::moment::sections(daw, project)
            .last()
            .map_or(600.0, |s| s.end)
    };
    let start = if span.0.is_finite() { span.0 } else { 0.0 };
    let (mut measure, _, _) = daw.time_to_musical(project.clone(), start.max(0.0));
    measure = measure.saturating_sub(1).max(1);
    let mut out = Vec::new();
    loop {
        let at = daw.musical_to_time(project.clone(), measure, 1, 0.0);
        let next = daw.musical_to_time(project.clone(), measure.saturating_add(1), 1, 0.0);
        let (beats, unit) = daw.get_time_signature_at(project.clone(), at);
        let beats = u32::try_from(beats.max(1)).unwrap_or(4);
        out.push(BarLine {
            bar: crate::song_bars::musical(daw, project, at + 1e-6).0,
            at,
            beats,
            unit: u32::try_from(unit.max(1)).unwrap_or(4),
            beat: (next - at) / f64::from(beats),
        });
        measure = measure.saturating_add(1);
        if at > end + 2.0 * (next - at) || out.len() > 10_000 || next <= at {
            break;
        }
    }
    out
}

fn settings_of(a: &PlanArg) -> Settings {
    Settings {
        grid_hint: a.grid_hint.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        anchors: a
            .anchors
            .iter()
            .map(|x| Anchor {
                piece: x.piece.clone(),
                beats: x.beats.clone(),
                section: x.section.clone(),
            })
            .collect(),
        lock_runs: a.lock_runs.unwrap_or(true),
        prefer: match a.prefer.as_deref() {
            Some(p) if p.eq_ignore_ascii_case("grid") => Prefer::Grid,
            _ => Prefer::Midi,
        },
        plan_fills: a.plan_fills.unwrap_or(false),
    }
}

/// A reason's kind, its id or grid left out: `rides`, `fill-grid`, …
const fn kind(reason: &Reason) -> &'static str {
    match reason {
        Reason::Midi => "midi",
        Reason::Grid => "grid",
        Reason::DoubleRun => "double-run",
        Reason::Rides(_) => "rides",
        Reason::FlamGrace(_) => "flam-grace",
        Reason::FillGrid(_) => "fill-grid",
        Reason::Kept => "kept",
        Reason::FalseDouble => "false-double",
        Reason::SeamDouble => "seam-double",
    }
}

/// The answer, compact: well under the MCP's 100 KB for a whole song
/// (the per-hit columns and the apply shape only when asked).
fn compact(
    plan: &Plan,
    request: &PlanRequest,
    at: &dyn Fn(f64) -> String,
    targets: bool,
    detail: bool,
) -> Json {
    let mut reasons: BTreeMap<&str, u32> = BTreeMap::new();
    for h in &plan.hits {
        *reasons.entry(kind(&h.reason)).or_default() += 1;
    }
    let doubts: Vec<Json> = plan
        .doubts
        .iter()
        .map(|d| {
            json!({
                "id": d.id,
                "piece": d.piece,
                "at": at(d.time),
                "candidates": [round(d.candidates[0], 6), round(d.candidates[1], 6)],
                "to": [at(d.candidates[0]), at(d.candidates[1])],
                "why": d.why,
            })
        })
        .collect();
    let mut by_bar: BTreeMap<i32, [Option<f64>; 2]> = BTreeMap::new();
    for l in &plan.leans {
        by_bar.entry(l.bar).or_default()[usize::from(l.half.min(1))] = Some(l.lean);
    }
    let lo = request
        .hits
        .iter()
        .map(|h| h.time)
        .fold(f64::INFINITY, f64::min);
    let hi = request
        .hits
        .iter()
        .map(|h| h.time)
        .fold(f64::NEG_INFINITY, f64::max);
    let in_span = |bar: i32| {
        request.bars.iter().any(|b| {
            b.bar == bar
                && (request.hits.is_empty()
                    || (b.at <= hi && b.at + f64::from(b.beats) * b.beat >= lo))
        })
    };
    let leans: Vec<Json> = by_bar
        .iter()
        .filter(|(b, _)| in_span(**b))
        .map(|(b, [l0, l1])| {
            json!([
                b,
                l0.map(|l| (l * 1000.0).round()),
                l1.map(|l| (l * 1000.0).round())
            ])
        })
        .collect();
    let mut out = json!({
        "hits": plan.hits.len(),
        "targets": plan.targets.len(),
        "rides": plan.rides.len(),
        "remove": plan.remove,
        "delete": plan.delete,
        "reasons": reasons,
        "doubts": doubts,
        "leans": leans,
    });
    if targets {
        let mut shape = apply_shape(plan);
        if let Some(t) = shape.get_mut("targets").and_then(Json::as_object_mut) {
            for v in t.values_mut() {
                if let Some(x) = v.as_f64() {
                    *v = json!(round(x, 6));
                }
            }
        }
        out["apply"] = shape;
    }
    if detail {
        out["rows"] = json!({
            "id": plan.hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
            "target": plan.hits.iter().map(|h| round(h.target, 6)).collect::<Vec<_>>(),
            "reason": plan.hits.iter().map(|h| h.reason.to_string()).collect::<Vec<_>>(),
            "confidence": plan.hits.iter().map(|h| round(h.confidence, 2)).collect::<Vec<_>>(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use daw_edit::plan::{Doubt, Lean, Planned};

    fn hit(id: &str, piece: &str, time: f64) -> Hit {
        Hit {
            id: id.to_owned(),
            piece: piece.to_owned(),
            time,
            peak_db: -20.0,
            from_trigger: true,
            from_mic: false,
            ghost: false,
            bleed: false,
            flam: false,
            unsure: false,
            false_double: true,
        }
    }

    #[test]
    fn a_hit_carries_its_take_and_flags() {
        let played = vec![
            hit("e5.kick.0001@-1500~420134", "Kick", 2.0),
            hit("x", "Kick", 9.0),
        ];
        let hits = played_in(&played, (1.98, 5.0));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].take, "420134");
        assert_eq!(hits[0].heard, Heard::Trigger);
        assert_eq!(hits[0].flags, vec!["false-double".to_owned()]);
    }

    #[test]
    fn a_span_is_its_sections_or_from_to() {
        let s = |n: &str, a: f64, b: f64| Section {
            name: n.into(),
            from: a,
            to: b,
        };
        let sections = vec![s("VS 1", 13.5, 61.5), s("CH 1", 73.5, 85.5)];
        assert_eq!(
            span_of(&sections, &["ch 1".into(), "VS 1".into()], None, None).unwrap(),
            (13.5, 85.5)
        );
        assert_eq!(
            span_of(&sections, &[], Some(1.0), None).unwrap(),
            (1.0, f64::INFINITY)
        );
        assert!(span_of(&sections, &["Bridge".into()], None, None).is_err());
    }

    #[test]
    fn the_map_is_one_pitch_per_piece() {
        let n = |time: f64, pitch: u8| crate::drum_reference::RefNote {
            time,
            pitch,
            velocity: 100,
        };
        // Kick on 24 (eight notes), a cymbal on 49 over four of them,
        // snare on 26.
        let mut notes = Vec::new();
        let mut hits = Vec::new();
        let played = |id: String, piece: &str, time: f64| PlayedHit {
            id,
            piece: piece.into(),
            take: String::new(),
            time,
            peak_db: -20.0,
            heard: Heard::Both,
            flags: Vec::new(),
        };
        for i in 0..8 {
            let t = 2.0 + f64::from(i) * 0.75;
            notes.push(n(t, 24));
            if i % 2 == 0 {
                notes.push(n(t, 49));
            }
            notes.push(n(t + 0.375, 26));
            hits.push(played(format!("k{i}"), "Kick", t + 0.01));
            hits.push(played(format!("s{i}"), "Snare", t + 0.385));
        }
        let mapped = mapped_notes(&notes, &hits, &HashMap::new(), (0.0, 100.0));
        assert_eq!(
            mapped.iter().filter(|m| m.piece == "Kick").count(),
            8,
            "the cymbal is not a kick"
        );
        assert_eq!(mapped.iter().filter(|m| m.piece == "Snare").count(), 8);
        let given = HashMap::from([("Snare".to_owned(), 24_u8)]);
        let mapped = mapped_notes(&notes, &hits, &given, (0.0, 100.0));
        assert!(mapped.iter().all(|m| m.piece == "Snare") && mapped.len() == 8);
    }

    #[test]
    fn bar_beat_is_the_owners_address() {
        assert_eq!(bar_beat((118, 4, 0.25)), "118.4.25");
        assert_eq!(bar_beat((150, 1, 0.0)), "150.1.00");
        assert_eq!(bar_beat((61, 1, 0.999)), "61.2.00");
    }

    #[test]
    fn the_answer_is_compact_and_in_applys_shape() {
        let plan = Plan {
            hits: vec![
                Planned {
                    id: "k".into(),
                    target: 1.5,
                    reason: Reason::Midi,
                    confidence: 1.0,
                },
                Planned {
                    id: "s".into(),
                    target: 1.51,
                    reason: Reason::Rides("k".into()),
                    confidence: 0.9,
                },
            ],
            targets: BTreeMap::from([("k".to_owned(), 1.500_000_4)]),
            remove: vec![],
            delete: vec![],
            rides: BTreeMap::from([("s".to_owned(), "k".to_owned())]),
            doubts: vec![Doubt {
                id: "k".into(),
                piece: "Kick".into(),
                time: 1.49,
                candidates: [1.5, 1.593_75],
                why: "two lines nearly as near".into(),
            }],
            leans: vec![Lean {
                bar: 1,
                half: 0,
                lean: -0.012,
                anchors: 2,
            }],
        };
        let request = PlanRequest {
            hits: vec![],
            notes: vec![],
            bars: vec![BarLine {
                bar: 1,
                at: 1.5,
                beats: 4,
                unit: 4,
                beat: 0.375,
            }],
            sections: vec![],
            zones: vec![],
            settings: Settings::default(),
        };
        let at = |t: f64| format!("{t:.2}");
        let out = compact(&plan, &request, &at, true, true);
        assert_eq!(out["reasons"]["rides"], 1);
        assert_eq!(out["doubts"][0]["to"][1], "1.59");
        assert_eq!(out["apply"]["targets"]["k"], 1.5);
        assert_eq!(out["apply"]["rides"]["s"], "k");
        assert_eq!(out["rows"]["reason"][1], "rides k");
        assert_eq!(out["leans"][0][1], -12.0);
        let lean = compact(&plan, &request, &at, false, false);
        assert!(lean.get("apply").is_none() && lean.get("rows").is_none());
    }
}
