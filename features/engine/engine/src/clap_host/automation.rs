//! `com.fasttrackstudio.host-automation/1` over the song's automation: a
//! CLAP writes, reads and clears the lanes of its **own** parameters (the
//! FX's parameter envelopes on its track), several writes one undo step.
//!
//! - **Whose parameters.** A call names its caller (the plugin's
//!   `clap_plugin`), which the [`pump`](super::pump) and every call Studio
//!   makes into a plugin [`register`] with its FX and parameter ids, read
//!   while Studio holds that plugin's lock. A parameter id the caller's
//!   plugin does not have is refused.
//! - **Who wrote a point.** Studio remembers the points each plugin wrote
//!   (time and value, per lane); a point that is not one of them is the
//!   user's. `read` says so per point, and `KEEP_USER_EDITS` keeps them.
//! - **The user's edits** of a plugin's lanes are noticed by the pump
//!   ([`user_edits`]) and sent on the plugin's `automation` topic.
//! - **The deadlock rule.** A plugin calls from inside one of Studio's
//!   calls into it, its lock held. The work runs on a worker thread that
//!   never needs that lock, and the call waits for it.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use daw::service::{ProjectContext, TrackRef};
use daw::standalone::Standalone;
use daw::standalone::clap_ext::ExtError;
use daw::standalone::clap_ext::automation::{
    AutomationEdit, AutomationPoint, By, Caller, HostAutomation, LaneInfo, LaneMode, Shape,
    TimeRange, WriteFlags, WriteReport, merge_points,
};
use daw_proto::automation::{
    AddPointParams, Automation, EnvelopeLocation, EnvelopePoint, EnvelopeRef, EnvelopeShape,
    LaneParams, TimeRangeParams,
};
use daw_proto::primitives::{AutomationMode, PositionInSeconds};

/// How long a plugin's call waits for its work.
pub const WAIT: Duration = Duration::from_secs(10);

/// The furthest a lane reaches, seconds: what "to the end" means here.
const END_S: f64 = 1.0e7;

/// A plugin Studio knows: its FX, track and song, and its parameters' CLAP
/// ids in index order (the FX's parameter index is the position).
#[derive(Clone, Debug)]
pub struct Registered {
    pub fx_guid: String,
    pub track_guid: String,
    pub project: ProjectContext,
    pub ids: Vec<u32>,
}

#[derive(Default)]
struct Book {
    plugins: HashMap<Caller, Registered>,
    /// Per (FX, parameter index): the points its plugin wrote, (s, value).
    written: HashMap<(String, u32), Vec<(f64, f64)>>,
    /// Per (FX, parameter index): the lane as last seen, to notice an edit.
    seen: HashMap<(String, u32), Vec<(f64, f64)>>,
    /// Per caller: the undo step it has open.
    open: HashMap<Caller, String>,
}

static BOOK: Mutex<Option<Book>> = Mutex::new(None);

fn with_book<R>(f: impl FnOnce(&mut Book) -> R) -> R {
    let mut b = BOOK.lock().unwrap_or_else(PoisonError::into_inner);
    f(b.get_or_insert_with(Book::default))
}

/// Know `caller` as the plugin of FX `fx_guid` (on `track_guid` of
/// `project`), with these parameter ids.
pub fn register(caller: Caller, registered: Registered) {
    with_book(|b| {
        b.plugins.insert(caller, registered);
    });
}

/// Forget every plugin not among `live` FX.
pub fn retain(live: &[String]) {
    with_book(|b| b.plugins.retain(|_, r| live.contains(&r.fx_guid)));
}

/// The registered plugin of FX `fx_guid`.
#[must_use]
pub fn caller_of(fx_guid: &str) -> Option<Caller> {
    with_book(|b| {
        b.plugins
            .iter()
            .find(|(_, r)| r.fx_guid == fx_guid)
            .map(|(c, _)| *c)
    })
}

/// Studio's side of host-automation/1, one for every plugin.
pub struct StudioAutomation {
    daw: &'static Standalone,
}

impl StudioAutomation {
    #[must_use]
    pub const fn new(daw: &'static Standalone) -> Self {
        Self { daw }
    }
}

fn shape_of(s: Shape) -> EnvelopeShape {
    match s {
        Shape::Linear => EnvelopeShape::Linear,
        Shape::Square => EnvelopeShape::Square,
        Shape::Slow => EnvelopeShape::SlowStartEnd,
        Shape::Fast => EnvelopeShape::FastStart,
        Shape::Bezier => EnvelopeShape::Bezier,
    }
}

fn shape_from(s: EnvelopeShape) -> Shape {
    match s {
        EnvelopeShape::Square => Shape::Square,
        EnvelopeShape::SlowStartEnd => Shape::Slow,
        EnvelopeShape::FastStart | EnvelopeShape::FastEnd => Shape::Fast,
        EnvelopeShape::Bezier => Shape::Bezier,
        EnvelopeShape::Linear => Shape::Linear,
    }
}

const fn mode_of(m: AutomationMode) -> LaneMode {
    match m {
        AutomationMode::TrimRead => LaneMode::Trim,
        AutomationMode::Read => LaneMode::Read,
        AutomationMode::Touch => LaneMode::Touch,
        AutomationMode::Write => LaneMode::Write,
        AutomationMode::Latch | AutomationMode::LatchPreview => LaneMode::Latch,
        AutomationMode::Off => LaneMode::Off,
    }
}

/// Whether `(t, v)` is one of `written`.
fn by_plugin(written: &[(f64, f64)], t: f64, v: f64) -> bool {
    written
        .iter()
        .any(|(wt, wv)| (wt - t).abs() < 1e-6 && (wv - v).abs() < 1e-6)
}

fn location(r: &Registered, index: u32) -> EnvelopeLocation {
    EnvelopeLocation::new(
        TrackRef::Guid(r.track_guid.clone()),
        EnvelopeRef::FxParam {
            fx_guid: r.fx_guid.clone(),
            param_index: index,
        },
    )
}

/// The caller's plugin and the parameter index of `param_id`.
fn resolve(caller: Caller, param_id: u32) -> Result<(Registered, u32), ExtError> {
    let r = with_book(|b| b.plugins.get(&caller).cloned())
        .ok_or_else(|| ExtError::NotFound("a plugin Studio does not know".into()))?;
    let index = r
        .ids
        .iter()
        .position(|id| *id == param_id)
        .and_then(|i| u32::try_from(i).ok())
        .ok_or_else(|| ExtError::NotFound(format!("parameter {param_id}: not this plugin's")))?;
    Ok((r, index))
}

/// Run `f` on a worker, never on the calling thread (the caller may hold
/// its plugin's lock), and wait for it.
fn off_the_plugin<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ExtError> + Send + 'static,
) -> Result<T, ExtError> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("clap-automation".into())
        .spawn(move || {
            let _ = tx.send(f());
        })
        .map_err(|e| ExtError::Failed(format!("no thread to write on: {e}")))?;
    rx.recv_timeout(WAIT)
        .map_err(|_| ExtError::Failed("the song is busy: try again".into()))?
}

fn points_of(daw: &Standalone, r: &Registered, index: u32) -> Vec<EnvelopePoint> {
    Automation::points(daw, r.project.clone(), location(r, index))
}

fn said(points: &[EnvelopePoint], written: &[(f64, f64)]) -> Vec<AutomationPoint> {
    points
        .iter()
        .map(|p| {
            let t = p.time.as_seconds();
            AutomationPoint {
                t,
                value: p.value,
                shape: shape_from(p.shape),
                tension: (p.shape == EnvelopeShape::Bezier).then_some(p.tension),
                by: if by_plugin(written, t, p.value) {
                    By::Plugin
                } else {
                    By::User
                },
            }
        })
        .collect()
}

fn span(range: TimeRange) -> (f64, f64) {
    (range.from.max(0.0), range.to.min(END_S))
}

/// Delete the points of `[from, to)`.
fn delete_range(daw: &Standalone, r: &Registered, at: &EnvelopeLocation, range: TimeRange) {
    let (from, to) = span(range);
    if to <= from {
        return;
    }
    Automation::delete_points_in_range(
        daw,
        r.project.clone(),
        at.clone(),
        TimeRangeParams::new(
            PositionInSeconds::from_seconds(from),
            // `[from, to)`: the end is exclusive here, inclusive there.
            PositionInSeconds::from_seconds((to - 1e-9).max(from)),
        ),
    );
}

/// The lane as it is now, kept as seen (a write of ours is no user edit).
fn mark_seen(daw: &Standalone, r: &Registered, index: u32) {
    let now: Vec<(f64, f64)> = points_of(daw, r, index)
        .iter()
        .map(|p| (p.time.as_seconds(), p.value))
        .collect();
    with_book(|b| {
        b.seen.insert((r.fx_guid.clone(), index), now);
    });
}

impl HostAutomation for StudioAutomation {
    fn lanes(&self, caller: Caller) -> Result<Vec<LaneInfo>, ExtError> {
        let r = with_book(|b| b.plugins.get(&caller).cloned())
            .ok_or_else(|| ExtError::NotFound("a plugin Studio does not know".into()))?;
        let daw = self.daw;
        off_the_plugin(move || {
            let written = with_book(|b| b.written.clone());
            Ok(
                Automation::envelopes(daw, r.project.clone(), TrackRef::Guid(r.track_guid.clone()))
                    .into_iter()
                    .filter(|e| e.fx_guid.as_deref() == Some(r.fx_guid.as_str()))
                    .filter_map(|e| {
                        let index = e.param_index?;
                        let id = *r.ids.get(usize::try_from(index).ok()?)?;
                        let pts = points_of(daw, &r, index);
                        let mine = written
                            .get(&(r.fx_guid.clone(), index))
                            .map_or(&[][..], Vec::as_slice);
                        let user = pts
                            .iter()
                            .filter(|p| !by_plugin(mine, p.time.as_seconds(), p.value))
                            .count();
                        Some(LaneInfo {
                            param_id: id,
                            mode: mode_of(e.automation_mode),
                            shown: e.visible,
                            points: u32::try_from(pts.len()).unwrap_or(u32::MAX),
                            user_points: u32::try_from(user).unwrap_or(u32::MAX),
                        })
                    })
                    .collect(),
            )
        })
    }

    fn read(
        &self,
        caller: Caller,
        param_id: u32,
        range: TimeRange,
    ) -> Result<Vec<AutomationPoint>, ExtError> {
        let (r, index) = resolve(caller, param_id)?;
        let daw = self.daw;
        off_the_plugin(move || {
            let written = with_book(|b| b.written.get(&(r.fx_guid.clone(), index)).cloned())
                .unwrap_or_default();
            let points = points_of(daw, &r, index);
            if points.is_empty()
                && Automation::envelope(daw, r.project.clone(), location(&r, index)).is_none()
            {
                return Err(ExtError::NotFound(format!("parameter {param_id}: no lane")));
            }
            Ok(said(&points, &written)
                .into_iter()
                .filter(|p| range.contains(p.t))
                .collect())
        })
    }

    fn write(
        &self,
        caller: Caller,
        param_id: u32,
        range: TimeRange,
        points: &[AutomationPoint],
        flags: WriteFlags,
    ) -> Result<WriteReport, ExtError> {
        let (r, index) = resolve(caller, param_id)?;
        let daw = self.daw;
        let points = points.to_vec();
        let grouped = with_book(|b| b.open.contains_key(&caller));
        off_the_plugin(move || {
            let at = location(&r, index);
            let existing = points_of(daw, &r, index);
            let lane = Automation::envelope(daw, r.project.clone(), at.clone());
            let created = existing.is_empty() && lane.is_none();
            if created && !flags.has(WriteFlags::CREATE_LANE) {
                return Err(ExtError::NotFound(format!("parameter {param_id}: no lane")));
            }
            let key = (r.fx_guid.clone(), index);
            let written = with_book(|b| b.written.get(&key).cloned()).unwrap_or_default();
            let (merged, kept) = merge_points(
                &said(&existing, &written),
                range,
                &points,
                flags.has(WriteFlags::KEEP_USER_EDITS),
            );
            let label = "Write a plugin's automation";
            if !grouped {
                daw::service::Projects::begin_undo_block(daw, r.project.clone(), label);
            }
            delete_range(daw, &r, &at, range);
            let mut mine: Vec<(f64, f64)> = written
                .into_iter()
                .filter(|(t, _)| !range.contains(*t))
                .collect();
            let mut count = 0_u32;
            for p in merged.iter().filter(|p| range.contains(p.t)) {
                let _ = Automation::add_point(
                    daw,
                    r.project.clone(),
                    at.clone(),
                    AddPointParams::new(
                        PositionInSeconds::from_seconds(p.t.max(0.0)),
                        p.value,
                        shape_of(p.shape),
                    ),
                );
                if p.by == By::Plugin {
                    mine.push((p.t.max(0.0), p.value));
                    count = count.saturating_add(1);
                }
            }
            if flags.has(WriteFlags::SHOW_LANE) {
                Automation::set_visible(daw, r.project.clone(), at.clone(), true);
                let _ = Automation::set_lane(daw, r.project.clone(), at, LaneParams::own(64));
            }
            if !grouped {
                daw::service::Projects::end_undo_block(daw, r.project.clone(), label, None);
                crate::song_model::request_resync();
            }
            with_book(|b| {
                b.written.insert(key, mine);
            });
            mark_seen(daw, &r, index);
            Ok(WriteReport {
                written: count,
                kept,
                created,
            })
        })
    }

    fn clear(&self, caller: Caller, param_id: u32, range: TimeRange) -> Result<(), ExtError> {
        let (r, index) = resolve(caller, param_id)?;
        let daw = self.daw;
        let grouped = with_book(|b| b.open.contains_key(&caller));
        off_the_plugin(move || {
            let label = "Clear a plugin's automation";
            if !grouped {
                daw::service::Projects::begin_undo_block(daw, r.project.clone(), label);
            }
            delete_range(daw, &r, &location(&r, index), range);
            if !grouped {
                daw::service::Projects::end_undo_block(daw, r.project.clone(), label, None);
                crate::song_model::request_resync();
            }
            with_book(|b| {
                if let Some(w) = b.written.get_mut(&(r.fx_guid.clone(), index)) {
                    w.retain(|(t, _)| !range.contains(*t));
                }
            });
            mark_seen(daw, &r, index);
            Ok(())
        })
    }

    fn begin_edit(&self, caller: Caller, label: &str) -> Result<(), ExtError> {
        let r = with_book(|b| b.plugins.get(&caller).cloned())
            .ok_or_else(|| ExtError::NotFound("a plugin Studio does not know".into()))?;
        let label = if label.trim().is_empty() {
            "A plugin's automation".to_owned()
        } else {
            label.to_owned()
        };
        if with_book(|b| b.open.contains_key(&caller)) {
            return Err(ExtError::Failed("an edit is already open".into()));
        }
        with_book(|b| b.open.insert(caller, label.clone()));
        let daw = self.daw;
        off_the_plugin(move || {
            daw::service::Projects::begin_undo_block(daw, r.project.clone(), &label);
            Ok(())
        })
    }

    fn end_edit(&self, caller: Caller) -> Result<(), ExtError> {
        let r = with_book(|b| b.plugins.get(&caller).cloned())
            .ok_or_else(|| ExtError::NotFound("a plugin Studio does not know".into()))?;
        let label = with_book(|b| b.open.remove(&caller))
            .ok_or_else(|| ExtError::Failed("no edit is open".into()))?;
        let daw = self.daw;
        off_the_plugin(move || {
            daw::service::Projects::end_undo_block(daw, r.project.clone(), &label, None);
            crate::song_model::request_resync();
            Ok(())
        })
    }
}

/// The user's edits of registered plugins' lanes since they were last
/// seen: `(plugin, edit)`, the edit's span from the first point that
/// differs to the last. A lane never seen is only seen. The pump's,
/// without any plugin's lock.
#[must_use]
pub fn user_edits(daw: &Standalone) -> Vec<(Caller, AutomationEdit)> {
    let plugins: Vec<(Caller, Registered)> =
        with_book(|b| b.plugins.iter().map(|(c, r)| (*c, r.clone())).collect());
    let mut out = Vec::new();
    for (caller, r) in plugins {
        let lanes: Vec<u32> =
            Automation::envelopes(daw, r.project.clone(), TrackRef::Guid(r.track_guid.clone()))
                .into_iter()
                .filter(|e| e.fx_guid.as_deref() == Some(r.fx_guid.as_str()))
                .filter_map(|e| e.param_index)
                .collect();
        for index in lanes {
            let now: Vec<(f64, f64)> = points_of(daw, &r, index)
                .iter()
                .map(|p| (p.time.as_seconds(), p.value))
                .collect();
            let key = (r.fx_guid.clone(), index);
            let was = with_book(|b| b.seen.insert(key, now.clone()));
            let Some(was) = was else { continue };
            if was == now {
                continue;
            }
            let differs: Vec<f64> = now
                .iter()
                .filter(|p| !was.contains(p))
                .chain(was.iter().filter(|p| !now.contains(p)))
                .map(|p| p.0)
                .collect();
            let (from, to) = differs
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), t| {
                    (lo.min(*t), hi.max(*t))
                });
            let Some(&id) = usize::try_from(index).ok().and_then(|i| r.ids.get(i)) else {
                continue;
            };
            if from.is_finite() {
                out.push((
                    caller,
                    AutomationEdit {
                        param_id: id,
                        from,
                        to: to.max(from),
                        by: By::User,
                    },
                ));
            }
        }
    }
    out
}
