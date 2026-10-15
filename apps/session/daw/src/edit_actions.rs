//! Editing what is selected: `EDIT_QUANTIZE`, `EDIT_REVERT`, `EDIT_LEVEL`
//! and `EDIT_SCOPE`.
//!
//! Select the drums — the kit's folder or a piece's, its folded rows, a
//! razor over them, a stretch of time — and quantize, level or put back
//! what is selected. The selection is an [`editing::scope::Scope`]: the
//! arrangement publishes its own every frame ([`publish`]); a model gives
//! one in the call (`tracks`, `spans`, `from`/`to`, `sections`), which
//! wins. Its tracks pick the profile ([`editing::scope::profile_of`]),
//! and the profile says what is cut as one: for the drums, the whole kit
//! — every mic of every piece, from its hit markers — over the selected
//! stretches, whichever of the kit's tracks are selected; leveling, only
//! the selected pieces.
//!
//! Design: `docs/design/editing.md`. The drum-named actions
//! (`QUANTIZE`, `DRUMS_REVERT`, `DRUMS_LEVEL`) are what these run for the
//! drums, and stay for the calls that name them.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::ProjectContext;
use editing::scope::{Node, Profile, Scope, Span};
use facet::Facet;

/// The arrangement's selection as an edit's scope, as last drawn.
static CURRENT: std::sync::Mutex<Option<Scope>> = std::sync::Mutex::new(None);

/// Publish the arrangement's selection (every frame it draws).
pub fn publish(scope: Scope) {
    if let Ok(mut slot) = CURRENT.lock() {
        *slot = Some(scope);
    }
}

/// The arrangement's selection as last drawn, if it has drawn.
#[must_use]
pub fn current() -> Option<Scope> {
    CURRENT.lock().ok().and_then(|slot| slot.clone())
}

static SCOPE: ActionMeta = crate::actions::meta(
    "EDIT_SCOPE",
    "edit_scope",
    "What an edit would act on",
    "What EDIT_QUANTIZE, EDIT_REVERT and EDIT_LEVEL would act on: the selection (or the call's `tracks`, `spans`, `from`/`to`, `sections`), its tracks by name with folders standing for what is under them, its stretches of time, and the profile its tracks are edited with (only `drums` so far). Read-only.",
    "Edit",
    "",
);

static QUANTIZE: ActionMeta = crate::actions::meta(
    "EDIT_QUANTIZE",
    "edit_quantize",
    "Quantize the selection",
    "Quantize what is selected — the arrangement's selection, or the call's `tracks` (names, paths or guids; a folder stands for what is under it), `spans` / `from`/`to` (seconds; none: the whole song) or `sections` — with the profile its tracks are edited with. Drums: the whole kit, every mic of every piece cut as one from its hit markers (HITS_DETECT first), over the selected stretches, as QUANTIZE does; the settings are QUANTIZE's (`grid`, `target`, `strength`, `fills`, `lead`, …; the sticky ones when left out), with `lock`, `dry_run` and `brief`. Says the profile, what it acted on and QUANTIZE's answer, its check included.",
    "Edit",
    "",
);

static REVERT: ActionMeta = crate::actions::meta(
    "EDIT_REVERT",
    "edit_revert",
    "Put the selection back as played",
    "Put what is selected back as played — the selection, or the call's `tracks`, `spans` / `from`/`to` or `sections` — with the profile its tracks are edited with. Drums: the whole kit over each stretch, as DRUMS_REVERT does: the hit markers kept to edit by hand, and (`hold`, on unless false) a no-quantize zone over it so a later quantize leaves it. A stretch of time is needed: the whole song is not put back by accident.",
    "Edit",
    "",
);

static LEVEL: ActionMeta = crate::actions::meta(
    "EDIT_LEVEL",
    "edit_level",
    "Level the selection",
    "Level what is selected — the selection, or the call's `tracks`, `spans` / `from`/`to` or `sections` — with the profile its tracks are edited with. Drums: each selected piece's strokes (every piece when only time is selected) brought toward one another, as DRUMS_LEVEL does — a volume curve on its close mics, the same gain on each; with stretches, only the strokes in them are leveled and the curve outside is kept. `consistency` 0–100 % (each drum's own when left out), `range_db`, `whole_song`, `dry_run`.",
    "Edit",
    "",
);

/// Where a call says to act, over the selection: any of these given and
/// the selection is not read.
#[derive(Facet, Clone, Debug, Default)]
pub(crate) struct On {
    /// Tracks by name, path or guid; a folder stands for what is under it.
    #[facet(default)]
    tracks: Vec<String>,
    #[facet(default)]
    spans: Vec<Span>,
    from: Option<f64>,
    to: Option<f64>,
    #[facet(default)]
    sections: Vec<String>,
}

impl On {
    fn given(&self) -> bool {
        !self.tracks.is_empty()
            || !self.spans.is_empty()
            || self.from.is_some()
            || self.to.is_some()
            || !self.sections.is_empty()
    }
}

/// What an edit acts on, resolved.
struct Target {
    profile: Profile,
    scope: Scope,
    sections: Vec<String>,
    /// The song's tracks, for naming and expanding the scope's.
    tracks: Vec<daw_proto::Track>,
}

impl Target {
    fn tree(&self) -> Vec<Node<'_>> {
        tree(&self.tracks)
    }

    /// Every track the scope stands for, folders opened.
    fn expanded(&self) -> Vec<String> {
        editing::scope::expand(&self.tree(), &self.scope.tracks)
    }

    fn said(&self) -> Said {
        let name = |guid: &String| {
            self.tracks
                .iter()
                .find(|t| t.guid == *guid)
                .map_or_else(|| guid.clone(), |t| t.name.clone())
        };
        Said {
            profile: self.profile.name().to_owned(),
            tracks: self.scope.tracks.iter().map(name).collect(),
            spans: self.scope.merged(),
            sections: self.sections.clone(),
        }
    }
}

fn tree(tracks: &[daw_proto::Track]) -> Vec<Node<'_>> {
    tracks
        .iter()
        .map(|t| Node {
            guid: &t.guid,
            name: &t.name,
            parent: t.parent_guid.as_deref(),
        })
        .collect()
}

/// The call's scope, else the arrangement's selection, and its profile.
fn target(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    on: &On,
) -> Result<Target, String> {
    let (scope, sections) = if on.given() {
        let tracks = on
            .tracks
            .iter()
            .map(|t| crate::actions::resolve(daw, project, t))
            .collect::<Result<Vec<String>, String>>()?;
        let mut spans = on.spans.clone();
        if on.from.is_some() || on.to.is_some() {
            spans.push(Span::between(
                on.from.unwrap_or(0.0),
                on.to.unwrap_or(f64::INFINITY),
            ));
        }
        (Scope { tracks, spans }, on.sections.clone())
    } else {
        let scope = current().filter(|s| !s.is_empty()).ok_or_else(|| {
            "nothing selected: select tracks, rows or a stretch of time — or say `tracks`, `from`/`to` or `sections`"
                .to_owned()
        })?;
        (scope, Vec::new())
    };
    let tracks = daw::service::Tracks::all(daw, project.clone());
    let profile = editing::scope::profile_of(&tree(&tracks), &scope)?;
    Ok(Target {
        profile,
        scope,
        sections,
        tracks,
    })
}

/// What an edit acted on.
#[derive(Facet, Clone, Debug, Default)]
pub(crate) struct Said {
    pub(crate) profile: String,
    /// The tracks as selected, by name (none: time alone).
    pub(crate) tracks: Vec<String>,
    /// The stretches, merged (none: the whole song, or the sections).
    pub(crate) spans: Vec<Span>,
    pub(crate) sections: Vec<String>,
}

#[derive(Facet)]
struct QuantizeArg {
    #[facet(flatten, default)]
    on: On,
    /// Settings for this quantize alone, over the sticky ones.
    #[facet(flatten, default)]
    with: crate::drum_settings::Patch,
    lock: Option<String>,
    dry_run: Option<bool>,
    brief: Option<bool>,
}

#[derive(Facet)]
struct Quantized {
    on: Said,
    spans: Vec<crate::actions::QuantizeSpan>,
}

#[derive(Facet)]
struct RevertArg {
    #[facet(flatten, default)]
    on: On,
    hold: Option<bool>,
}

#[derive(Facet)]
struct LevelArg {
    #[facet(flatten, default)]
    on: On,
    /// How consistent, percent: 100 every stroke one level.
    consistency: Option<f64>,
    /// The most a stroke moves, dB.
    range_db: Option<f64>,
    whole_song: Option<bool>,
    dry_run: Option<bool>,
}

#[derive(Facet)]
struct Leveled {
    on: Said,
    pieces: Vec<crate::drum_level::Leveled>,
}

/// The kit's pieces the scope's tracks hold: a piece is selected when any
/// of its tracks is (a folder standing for what is under it). No tracks:
/// every piece.
fn pieces(daw: &daw::standalone::Standalone, project: &ProjectContext, on: &Target) -> Vec<String> {
    if on.scope.tracks.is_empty() {
        return Vec::new();
    }
    let selected = on.expanded();
    let (kit, _) = crate::hits::kit(daw, project);
    kit.iter()
        .filter(|p| p.tracks().any(|t| selected.contains(t)))
        .map(|p| p.name.clone())
        .collect()
}

/// QUANTIZE's spans and plan for a scope: its sections, one span, or a
/// step for each of several.
fn quantize_spans(
    on: &Target,
) -> (
    Vec<String>,
    Option<(f64, f64)>,
    Vec<crate::actions::PlanStep>,
) {
    if !on.sections.is_empty() {
        return (on.sections.clone(), None, Vec::new());
    }
    match on.scope.merged().as_slice() {
        [] => (Vec::new(), None, Vec::new()),
        [one] => (Vec::new(), Some((one.from, one.to)), Vec::new()),
        many => (
            Vec::new(),
            None,
            many.iter()
                .map(|s| crate::actions::PlanStep {
                    with: crate::drum_settings::Patch::default(),
                    sections: Vec::new(),
                    from: Some(s.from),
                    to: Some(s.to),
                    lock: None,
                })
                .collect(),
        ),
    }
}

// r[impl quantize.selection]
/// `EDIT_QUANTIZE`: see its description.
fn quantize(a: QuantizeArg) -> Result<Quantized, String> {
    let (daw, project) = crate::actions::song()?;
    let on = target(daw, &project, &a.on)?;
    match on.profile {
        Profile::Drums => {
            let (sections, span, plan) = quantize_spans(&on);
            let plan = plan
                .into_iter()
                .map(|step| crate::actions::PlanStep {
                    lock: a.lock.clone(),
                    ..step
                })
                .collect();
            let spans = crate::actions::quantize(crate::actions::QuantizeArg {
                with: a.with,
                sections,
                from: span.map(|s| s.0),
                to: span.map(|s| s.1),
                dry_run: a.dry_run,
                lock: a.lock,
                brief: a.brief,
                plan,
            })?;
            Ok(Quantized {
                on: on.said(),
                spans,
            })
        }
    }
}

/// `EDIT_REVERT`: see its description.
fn revert(a: RevertArg) -> Result<Quantized, String> {
    let (daw, project) = crate::actions::song()?;
    let on = target(daw, &project, &a.on)?;
    match on.profile {
        Profile::Drums => {
            if on.sections.is_empty() && on.scope.merged().is_empty() {
                return Err(
                    "select a stretch of time, or rows, to put back as played: the whole song is not put back"
                        .to_owned(),
                );
            }
            let mut spans = Vec::new();
            let stretches: Vec<(Vec<String>, Option<f64>, Option<f64>)> = if on.sections.is_empty()
            {
                on.scope
                    .merged()
                    .iter()
                    .map(|s| (Vec::new(), Some(s.from), Some(s.to)))
                    .collect()
            } else {
                vec![(on.sections.clone(), None, None)]
            };
            for (sections, from, to) in stretches {
                spans.extend(crate::actions::revert(crate::actions::RevertArg {
                    sections,
                    from,
                    to,
                    hold: a.hold,
                })?);
            }
            Ok(Quantized {
                on: on.said(),
                spans,
            })
        }
    }
}

/// `EDIT_LEVEL`: see its description.
fn level(a: LevelArg) -> Result<Leveled, String> {
    let (daw, project) = crate::actions::song()?;
    let on = target(daw, &project, &a.on)?;
    match on.profile {
        Profile::Drums => {
            let pieces = pieces(daw, &project, &on);
            if !on.scope.tracks.is_empty() && pieces.is_empty() {
                return Err(
                    "no piece of the kit is selected (or none has hit markers: HITS_DETECT)"
                        .to_owned(),
                );
            }
            let knobs = crate::drum_level::Knobs {
                consistency: a.consistency.map(|c| (c / 100.0).clamp(0.0, 1.0)),
                range_db: a.range_db,
                whole_song: a.whole_song.unwrap_or(false),
            };
            let spans: Vec<(f64, f64)> = on.scope.merged().iter().map(|s| (s.from, s.to)).collect();
            let leveled = crate::drum_level::level(
                daw,
                &project,
                (&pieces, &spans),
                (&knobs, a.dry_run.unwrap_or(false)),
            )?;
            Ok(Leveled {
                on: on.said(),
                pieces: leveled,
            })
        }
    }
}

pub fn register(list: &ActionList) {
    list.register(&SCOPE, Effect::ReadOnly, |on: On| {
        let (daw, project) = crate::actions::song()?;
        Ok(target(daw, &project, &on)?.said())
    });
    list.register(&QUANTIZE, Effect::Mutating, quantize);
    list.register(&REVERT, Effect::Mutating, revert);
    list.register(&LEVEL, Effect::Mutating, level);
}
