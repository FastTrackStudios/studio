//! `DRUMS_APPLY`: a whole drum edit in one step.
//!
//! An agent (or the owner, through `DRUMS_FIX`) says where every hit
//! should play — `targets`, by hit id — which markers go (`remove`),
//! which strokes' sound goes (`delete`) and which hits keep their played
//! distance from another (`rides`). The engine solves the whole set at
//! once ([`solve`]: the moments the placement moves together, rides,
//! order and overlaps, the stretch limit), refuses what it cannot do
//! without a choice (`conflicts`), and writes the rest as one placement
//! of the kit ([`crate::hit_place::plan`]), one undo step under a mark.
//!
//! The stretch limit: a slice is never drawn out slower than
//! [`MIN_RATE`] (the owner's rule after a slice played at 0.11×, bar
//! 128). Where hits moving apart would need more, what follows is moved
//! up to meet it (`moved_up`) instead. A placement only shortens a slice
//! by cutting it, never by speeding it up, so [`MAX_RATE`] holds by
//! construction; the report reads every slice's rate back from the lanes
//! and says any outside either.
//!
//! The pure part ([`solve`]) is tested on its own and timed by
//! `drum-bench`; the rest reads the song, writes it and checks it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::ProjectContext;
use daw::standalone::item_ops::ItemOp;
use daw_proto::id::ItemId;
use facet::Facet;

use crate::actions::meta;

/// The slowest a slice may be drawn out: 0.8 of its speed.
pub const MIN_RATE: f64 = 0.8;
/// The fastest a slice may play: 1.25 times.
pub const MAX_RATE: f64 = 1.25;
/// Two hits of one drum this close after an edit are on one line, s.
pub const SAME_LINE: f64 = 0.010;
/// A shift smaller than this is no move, s.
const NO_MOVE: f64 = 0.000_05;
/// Two targets for hits struck together this far apart disagree, s.
const DISAGREE: f64 = 0.002;
/// How long a slice's attack plays as recorded before a draw-out slows
/// it ([`crate::stretch::STRETCH_KEEP`]), s.
const KEEP: f64 = crate::stretch::STRETCH_KEEP;
/// How far after its leader a rider is looked for when the leader is
/// moved up, s.
const RIDE_REACH: f64 = 0.5;
/// How many landing errors a result lists unless asked for all.
const LANDINGS: usize = 60;
/// A landing nearer its target than this is not listed, ms.
const LANDED_MS: f64 = 0.5;

// ── the solve: pure ───────────────────────────────────────────────────

/// One hit of the kit as it plays now.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub id: String,
    pub piece: String,
    /// Where it plays now, project seconds.
    pub now: f64,
    /// Where it was recorded (where the COMP lane plays it), seconds.
    pub recorded: f64,
    pub peak_db: f64,
    /// The other stroke of its flam (`HITS_MARK flam_with`/`grace_of`).
    pub flam_with: Option<String>,
}

/// What an edit asks for.
#[derive(Clone, Debug, Default)]
pub struct Edit {
    /// Hit id → where it should play, project seconds.
    pub targets: BTreeMap<String, f64>,
    /// Hits whose markers go: no cut there, the sound rides the slice
    /// before.
    pub remove: BTreeSet<String>,
    /// Hits whose sound goes: their slice cut out, the slice before
    /// drawn out to the next.
    pub delete: BTreeSet<String>,
    /// Hit id → its leader's id: it keeps its played distance from it.
    pub rides: BTreeMap<String, String>,
}

/// How the solve cuts and joins, seconds.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The kit's moment: hits this close play as one and move together.
    pub merge: f64,
    /// The closest two moments may land: a piece's lead and crossfade
    /// before an attack must not reach the attack before.
    pub apart: f64,
    /// How far past the attack before it a slice reaching back to meet
    /// the one before may start, in its recording: the placement's guard
    /// and crossfade.
    pub guard: f64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            merge: editing::drums::KIT_MERGE,
            apart: 0.012,
            guard: 0.020,
        }
    }
}

/// Hits the placement moves together, and where they go.
#[derive(Clone, Debug, PartialEq)]
pub struct Moment {
    /// Its first hit's time now and as recorded.
    pub now: f64,
    pub recorded: f64,
    /// Its hits, as indices into the solve's strokes.
    pub hits: Vec<usize>,
    /// How far it moves, seconds.
    pub by: f64,
    /// Its sound goes.
    pub deleted: bool,
    /// A target of its own was given.
    pub targeted: bool,
}

impl Moment {
    /// Where it plays after the edit.
    #[must_use]
    pub fn lands(&self) -> f64 {
        self.now + self.by
    }
}

/// What the solve could not do without a choice: refused unless a ride
/// or a delete covers it.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Conflict {
    /// `split` (hits struck together sent apart), `order` (a hit sent
    /// past another), `swallow` (sent onto another's attack), `line`
    /// (two hits of one drum on one line), `partial-delete`.
    pub kind: String,
    pub ids: Vec<String>,
    /// Where, project seconds as it plays now.
    pub at: f64,
    pub why: String,
}

/// A moment moved up to keep the slice before it from playing slower
/// than [`MIN_RATE`].
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct MovedUp {
    pub id: String,
    /// How much earlier than asked it lands, ms.
    pub by_ms: f64,
}

/// The whole set solved.
#[derive(Clone, Debug, Default)]
pub struct Solution {
    pub strokes: Vec<Stroke>,
    /// By time, the deleted ones among them.
    pub moments: Vec<Moment>,
    pub conflicts: Vec<Conflict>,
    pub moved_up: Vec<MovedUp>,
    /// Ids the edit names that play nowhere now.
    pub unknown: Vec<String>,
    /// Each slice the placement must draw out: its moment's first hit and
    /// the rate it plays at (below 1: slower).
    pub drawn: Vec<(String, f64)>,
}

impl Solution {
    /// The moves the placement makes: every moment, `(now, by)`.
    #[must_use]
    pub fn moves(&self) -> Vec<(f64, f64)> {
        self.moments.iter().map(|m| (m.now, m.by)).collect()
    }

    /// The hits a piece starts at: every stroke the edit keeps a marker of
    /// (a deleted one included: its slice is cut, then cut out).
    #[must_use]
    pub fn cut_on(&self) -> Vec<f64> {
        self.strokes.iter().map(|s| s.now).collect()
    }

    /// How many moments move.
    #[must_use]
    pub fn moved(&self) -> usize {
        self.moments
            .iter()
            .filter(|m| !m.deleted && m.by.abs() > NO_MOVE)
            .count()
    }

    /// Where each hit that was given a target, or rides one, lands:
    /// `(stroke index, where it lands, where it was sent)`.
    #[must_use]
    pub fn sent(&self, edit: &Edit) -> Vec<(usize, f64, f64)> {
        let mut out = Vec::new();
        for m in self.moments.iter().filter(|m| !m.deleted) {
            for &k in &m.hits {
                let Some(s) = self.strokes.get(k) else {
                    continue;
                };
                let lands = s.now + m.by;
                let sent = edit.targets.get(&s.id).copied().unwrap_or(lands);
                if edit.targets.contains_key(&s.id) || m.by.abs() > NO_MOVE {
                    out.push((k, lands, sent));
                }
            }
        }
        out
    }
}

/// Solve `edit` over `strokes` (every hit that plays now): which moments
/// move how far, in one set.
#[must_use]
pub fn solve(strokes: &[Stroke], edit: &Edit, limits: Limits) -> Solution {
    let playing: BTreeSet<&str> = strokes.iter().map(|s| s.id.as_str()).collect();
    let unknown: Vec<String> = edit
        .targets
        .keys()
        .chain(&edit.remove)
        .chain(&edit.delete)
        .chain(edit.rides.keys())
        .chain(edit.rides.values())
        .filter(|id| !playing.contains(id.as_str()))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut kept: Vec<Stroke> = strokes
        .iter()
        .filter(|s| !edit.remove.contains(&s.id))
        .cloned()
        .collect();
    kept.sort_by(|a, b| a.now.total_cmp(&b.now));
    let mut out = Solution {
        moments: moments_of(&kept, limits.merge),
        strokes: kept,
        unknown,
        ..Solution::default()
    };
    let riders = riders(&out.strokes, edit);
    shifts(&mut out, edit, &riders);
    rides(&mut out, &riders);
    deletes(&mut out, edit);
    order(&mut out, edit, &riders, limits);
    stretch_limit(&mut out, &riders, limits.guard);
    out
}

/// The strokes, by time, as the placement groups them: each hit within
/// `merge` of its moment's first.
fn moments_of(strokes: &[Stroke], merge: f64) -> Vec<Moment> {
    let mut out: Vec<Moment> = Vec::new();
    for (k, s) in strokes.iter().enumerate() {
        match out.last_mut() {
            Some(m) if s.now - m.now <= merge => m.hits.push(k),
            _ => out.push(Moment {
                now: s.now,
                recorded: s.recorded,
                hits: vec![k],
                by: 0.0,
                deleted: false,
                targeted: false,
            }),
        }
    }
    out
}

/// Each riding stroke's leader, by stroke index: the edit's rides, and a
/// flam's quieter stroke riding the louder.
fn riders(strokes: &[Stroke], edit: &Edit) -> HashMap<usize, usize> {
    let index: HashMap<&str, usize> = strokes
        .iter()
        .enumerate()
        .map(|(k, s)| (s.id.as_str(), k))
        .collect();
    let mut out = HashMap::new();
    for (k, s) in strokes.iter().enumerate() {
        let Some(&main) = s.flam_with.as_deref().and_then(|m| index.get(m)) else {
            continue;
        };
        let louder = strokes.get(main).is_some_and(|m| m.peak_db >= s.peak_db);
        if louder && main != k {
            out.insert(k, main);
        }
    }
    for (rider, leader) in &edit.rides {
        if let (Some(&r), Some(&l)) = (index.get(rider.as_str()), index.get(leader.as_str()))
            && r != l
        {
            out.insert(r, l);
        }
    }
    out
}

/// Each moment's own shift from its targets (a rider's target is its
/// leader's business); hits struck together sent apart are a conflict.
fn shifts(out: &mut Solution, edit: &Edit, riders: &HashMap<usize, usize>) {
    let mut conflicts = Vec::new();
    for m in &mut out.moments {
        let asked: Vec<(usize, f64)> = m
            .hits
            .iter()
            .filter(|k| !riders.contains_key(k))
            .filter_map(|&k| {
                let s = out.strokes.get(k)?;
                edit.targets.get(&s.id).map(|t| (k, t - s.now))
            })
            .collect();
        let Some(&(_, first)) = asked.first() else {
            continue;
        };
        if let Some(&(k, other)) = asked.iter().find(|(_, by)| (by - first).abs() > DISAGREE) {
            let ids = vec![
                asked
                    .first()
                    .and_then(|(k, _)| out.strokes.get(*k))
                    .map(|s| s.id.clone())
                    .unwrap_or_default(),
                out.strokes.get(k).map(|s| s.id.clone()).unwrap_or_default(),
            ];
            conflicts.push(Conflict {
                kind: "split".to_owned(),
                at: m.now,
                why: format!(
                    "struck together (within {:.0} ms) they move together, but were sent {:.1} ms and {:.1} ms: give one target, or ride one on the other",
                    editing::drums::KIT_MERGE * 1_000.0,
                    first * 1_000.0,
                    other * 1_000.0
                ),
                ids,
            });
        }
        m.by = first;
        m.targeted = true;
    }
    out.conflicts.extend(conflicts);
}

/// Each moment holding a rider (and none of its own targets) moves as its
/// leader's does: chains followed, a few deep.
fn rides(out: &mut Solution, riders: &HashMap<usize, usize>) {
    let moment_of: HashMap<usize, usize> = out
        .moments
        .iter()
        .enumerate()
        .flat_map(|(i, m)| m.hits.iter().map(move |&k| (k, i)))
        .collect();
    for _ in 0..8 {
        let mut changed = false;
        for (rider, leader) in riders {
            let (Some(&r), Some(&l)) = (moment_of.get(rider), moment_of.get(leader)) else {
                continue;
            };
            if r == l {
                continue;
            }
            let Some(by) = out.moments.get(l).map(|m| m.by) else {
                continue;
            };
            if let Some(m) = out.moments.get_mut(r)
                && (m.by - by).abs() > NO_MOVE / 10.0
            {
                m.by = by;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// The moments whose sound goes: every hit of them deleted. One deleted
/// beside a hit it was struck with cannot be cut out alone.
fn deletes(out: &mut Solution, edit: &Edit) {
    let mut conflicts = Vec::new();
    for i in 0..out.moments.len() {
        let Some(m) = out.moments.get(i) else {
            continue;
        };
        let ids: Vec<&str> = m
            .hits
            .iter()
            .filter_map(|k| out.strokes.get(*k))
            .map(|s| s.id.as_str())
            .collect();
        let gone = ids.iter().filter(|id| edit.delete.contains(**id)).count();
        if gone == 0 {
            continue;
        }
        if gone < ids.len() {
            conflicts.push(Conflict {
                kind: "partial-delete".to_owned(),
                ids: ids.iter().map(|s| (*s).to_owned()).collect(),
                at: m.now,
                why: "a deleted hit shares its slice with a hit struck with it: delete them all, or remove its marker instead".to_owned(),
            });
            continue;
        }
        // Its cut stays with the slice before it, which is drawn out over it.
        let before = out.moments[..i]
            .iter()
            .rev()
            .find(|m| !m.deleted)
            .map_or(0.0, |m| m.by);
        if let Some(m) = out.moments.get_mut(i) {
            m.deleted = true;
            m.by = before;
        }
    }
    out.conflicts.extend(conflicts);
}

/// Whether stroke `a` rides `b` or `b` rides `a`.
fn riding(riders: &HashMap<usize, usize>, a: &[usize], b: &[usize]) -> bool {
    a.iter().any(|x| b.iter().any(|y| riders.get(x) == Some(y)))
        || b.iter().any(|y| a.iter().any(|x| riders.get(y) == Some(x)))
}

/// Order and overlaps: no moment sent past another, none onto another's
/// attack, no two hits of one drum newly on one line.
fn order(out: &mut Solution, edit: &Edit, riders: &HashMap<usize, usize>, limits: Limits) {
    let live: Vec<&Moment> = out.moments.iter().filter(|m| !m.deleted).collect();
    let id = |m: &Moment| -> String {
        m.hits
            .first()
            .and_then(|k| out.strokes.get(*k))
            .map(|s| s.id.clone())
            .unwrap_or_default()
    };
    let mut conflicts = Vec::new();
    for w in live.windows(2) {
        let [a, b] = w else { continue };
        let (was, will) = (b.now - a.now, b.lands() - a.lands());
        if riding(riders, &a.hits, &b.hits) && will >= was - NO_MOVE {
            continue;
        }
        if will <= 0.0 {
            conflicts.push(Conflict {
                kind: "order".to_owned(),
                ids: vec![id(a), id(b)],
                at: a.now,
                why: format!(
                    "sent past each other ({:.1} ms apart, played {:.1} ms apart): send both, or delete one",
                    will * 1_000.0,
                    was * 1_000.0
                ),
            });
        } else if will < limits.apart && will < was - NO_MOVE {
            conflicts.push(Conflict {
                kind: "swallow".to_owned(),
                ids: vec![id(a), id(b)],
                at: a.now,
                why: format!(
                    "{:.1} ms apart: the later's cut would take the earlier's attack (they need {:.0} ms): ride one on the other, or delete one",
                    will * 1_000.0,
                    limits.apart * 1_000.0
                ),
            });
        }
    }
    // Two hits of one drum newly on one line.
    let mut by_piece: BTreeMap<&str, Vec<(f64, f64, usize)>> = BTreeMap::new();
    for m in &live {
        for &k in &m.hits {
            if let Some(s) = out.strokes.get(k) {
                by_piece
                    .entry(s.piece.as_str())
                    .or_default()
                    .push((s.now + m.by, s.now, k));
            }
        }
    }
    for (piece, mut hits) in by_piece {
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        for w in hits.windows(2) {
            let [(la, na, ka), (lb, nb, kb)] = w else {
                continue;
            };
            let covered = riders.get(ka) == Some(kb) || riders.get(kb) == Some(ka);
            if lb - la < SAME_LINE && (nb - na).abs() >= SAME_LINE && !covered {
                let ids: Vec<String> = [ka, kb]
                    .iter()
                    .filter_map(|k| out.strokes.get(**k).map(|s| s.id.clone()))
                    .collect();
                let targeted = ids.iter().filter(|i| edit.targets.contains_key(*i)).count();
                conflicts.push(Conflict {
                    kind: "line".to_owned(),
                    ids,
                    at: *na,
                    why: format!(
                        "two {piece} hits {:.1} ms apart ({targeted} of them sent there): delete one, or send it elsewhere",
                        (lb - la) * 1_000.0
                    ),
                });
            }
        }
    }
    out.conflicts.extend(conflicts);
}

/// The farthest a slice whose recording runs `natural` seconds to the
/// next attack may be drawn out, no slower than [`MIN_RATE`] (its first
/// [`KEEP`] as recorded, as the draw-out does).
#[must_use]
pub fn farthest(natural: f64) -> f64 {
    farthest_at(natural, MIN_RATE)
}

/// [`farthest`], no slower than `least`.
#[must_use]
pub fn farthest_at(natural: f64, least: f64) -> f64 {
    let keep = KEEP.min(natural / 2.0);
    keep + (natural - keep) / least
}

/// The stretch limit on a placement's pieces (`daw.edit`'s answer, in
/// timeline order): a piece drawn out past what [`MIN_RATE`] reaches is
/// drawn no further, and the piece after it starts earlier to meet it —
/// playing its own recording before its attack, back to no nearer than
/// `guard` after the attack before (`attacks`, recording times, sorted),
/// where `joined` says that stretch of recording is one take's. Each
/// piece still drawn out: its start and the rate it plays at.
pub fn limit_draws(
    pieces: &mut [daw_edit::placement::Piece],
    attacks: &[f64],
    joined: &dyn Fn(f64, f64) -> bool,
    (guard, least): (f64, f64),
) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for k in 0..pieces.len() {
        let (Some(p), Some(n)) = (pieces.get(k).copied(), pieces.get(k + 1).copied()) else {
            continue;
        };
        let Some(reach) = p.drawn_to else { continue };
        let natural = (p.end - p.start).max(1e-6);
        let most = p.start + farthest_at(natural, least);
        let mut reach_to = reach;
        if reach > most + 1e-6 {
            let start_rec = n.start - n.shift;
            // Its own attack, and the one before it in the recording.
            let own = attacks.partition_point(|a| *a < start_rec - 1e-6);
            let before = own
                .checked_sub(1)
                .and_then(|i| attacks.get(i))
                .copied()
                .unwrap_or(f64::NEG_INFINITY);
            let earliest = before + guard + n.shift;
            let start = most.max(earliest);
            if start < n.start - 1e-6 && joined(start - n.shift, start_rec) {
                if let Some(next) = pieces.get_mut(k + 1) {
                    next.start = start;
                }
                reach_to = start;
            }
            if let Some(piece) = pieces.get_mut(k) {
                piece.drawn_to = (reach_to > piece.end + 1e-9).then_some(reach_to);
            }
        }
        let r = rate(natural, reach_to - p.start);
        if r < 1.0 - 1e-6 {
            out.push((p.start, r));
        }
    }
    out
}

/// The rate a slice whose recording runs `natural` seconds plays at over
/// `length` seconds: 1 when it is cut short, below 1 when drawn out.
#[must_use]
pub fn rate(natural: f64, length: f64) -> f64 {
    if length <= natural + 1e-9 {
        return 1.0;
    }
    let keep = KEEP.min(natural / 2.0);
    (natural - keep) / (length - keep).max(1e-9)
}

/// The stretch limit: in the order they play, the slice before a moment
/// is drawn out no slower than [`MIN_RATE`]; past that the moment's own
/// slice starts earlier to meet it (its recording before its attack, one
/// take's, back to `guard` after the attack before it: [`limit_draws`]
/// does it to the placement); and only past that too is the moment (and
/// what rides it) moved up.
fn stretch_limit(out: &mut Solution, riders: &HashMap<usize, usize>, guard: f64) {
    let n = out.moments.len();
    for i in 0..n {
        let Some(a) = out.moments.get(i).filter(|m| !m.deleted).cloned() else {
            continue;
        };
        // The recording runs to the next moment's attack, deleted or not.
        let Some(next_recorded) = out.moments.get(i + 1).map(|m| m.recorded) else {
            continue;
        };
        let Some(j) = (i + 1..n).find(|j| out.moments.get(*j).is_some_and(|m| !m.deleted)) else {
            continue;
        };
        let natural = next_recorded - a.recorded;
        let Some(b) = out.moments.get(j) else {
            continue;
        };
        let length = b.lands() - a.lands();
        if natural <= 0.0 {
            continue;
        }
        let most = farthest(natural);
        // How far back the next slice may reach in its own recording.
        let back = out.moments.get(j - 1).map_or(0.0, |p| {
            let same = take_of_moment(&out.strokes, p) == take_of_moment(&out.strokes, b);
            if same {
                (b.recorded - p.recorded - guard).max(0.0)
            } else {
                0.0
            }
        });
        if length > most + back + 1e-6 {
            let delta = length - most - back;
            let leader: Vec<usize> = b.hits.clone();
            let first = leader
                .first()
                .and_then(|k| out.strokes.get(*k))
                .map(|s| s.id.clone())
                .unwrap_or_default();
            // It and the later moments riding it (a ride is near) move up
            // together.
            let reach = b.now + RIDE_REACH;
            for (k, m) in out
                .moments
                .iter_mut()
                .enumerate()
                .skip(j)
                .take_while(|(_, m)| m.now <= reach)
            {
                let rides_it = k == j
                    || m.hits
                        .iter()
                        .any(|h| riders.get(h).is_some_and(|l| leader.contains(l)));
                if rides_it {
                    m.by -= delta;
                }
            }
            out.moved_up.push(MovedUp {
                id: first,
                by_ms: (delta * 10_000.0).round() / 10.0,
            });
        }
        let Some(b) = out.moments.get(j) else {
            continue;
        };
        let r = rate(natural, (b.lands() - a.lands()).min(most));
        if r < 1.0 - 1e-6 {
            let id = a
                .hits
                .first()
                .and_then(|k| out.strokes.get(*k))
                .map(|s| s.id.clone())
                .unwrap_or_default();
            out.drawn.push((id, (r * 1_000.0).round() / 1_000.0));
        }
    }
}

/// The take a moment's first hit is of (its id's code after `~`).
fn take_of_moment<'a>(strokes: &'a [Stroke], m: &Moment) -> &'a str {
    m.hits
        .first()
        .and_then(|k| strokes.get(*k))
        .map_or("", |s| crate::edit_list::take_of(&s.id))
}

/// A synthetic song's edit for timing the solve (`drum-bench`, the
/// tests): `n` hits at 160 bpm in sixteenths — a hat on each, the kick on
/// 1 and 3 with the hat, the snare on 2 and 4 with a flam's grace before
/// every eighth one — played up to ±20 ms off and sent to the grid.
#[must_use]
pub fn synthetic(n: usize) -> (Vec<Stroke>, Edit) {
    let sixteenth = 60.0 / 160.0 / 4.0;
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut jitter = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let unit = f64::from(u32::try_from(seed >> 40).unwrap_or(0)) / f64::from(1_u32 << 24);
        (unit - 0.5) * 0.040
    };
    let mut strokes = Vec::with_capacity(n);
    let mut edit = Edit::default();
    let mut line = 0_u32;
    while strokes.len() < n {
        let grid = 1.5 + f64::from(line) * sixteenth;
        let off = jitter();
        let played = grid + off;
        let slot = line % 16;
        let mut push = |name: &str, piece: &str, at: f64, peak: f64, flam: Option<String>| {
            let id = format!("{name}.{line}~bench");
            strokes.push(Stroke {
                id: id.clone(),
                piece: piece.to_owned(),
                now: at,
                recorded: at,
                peak_db: peak,
                flam_with: flam,
            });
            id
        };
        let lead = if slot == 4 || slot == 12 {
            let main = push("snare", "Snare", played, -6.0, None);
            if line % 128 == 4 {
                push("grace", "Snare", played - 0.035, -20.0, Some(main.clone()));
            }
            main
        } else if slot == 0 || slot == 8 {
            push("kick", "Kick", played, -3.0, None)
        } else {
            push("hat", "Hat", played, -12.0, None)
        };
        if slot % 2 == 0 && slot != 4 && slot != 12 {
            push("ride", "Ride", played + 0.004, -12.0, None);
        }
        edit.targets.insert(lead, grid);
        line = line.saturating_add(1);
    }
    strokes.truncate(n);
    strokes.sort_by(|a, b| a.now.total_cmp(&b.now));
    (strokes, edit)
}

// ── the action ────────────────────────────────────────────────────────

pub static APPLY: ActionMeta = meta(
    "DRUMS_APPLY",
    "drums_apply",
    "Apply a whole drum edit",
    "Write a whole drum edit in one step. `targets` {hit id: seconds}: where each hit plays (project seconds, as the song plays now); `remove` [ids]: their markers go (no cut there, the sound rides the slice before); `delete` [ids]: their sound goes (the slice cut out, the slice before drawn out to the next); `rides` {id: leader id}: the hit keeps its played distance from its leader (a flam's grace, a unison kick beside a snare, a second take at a comp seam; a flam marked with HITS_MARK rides by itself). Or `list`: a change list (DRUMS_EDIT_EXPORT, `studio.drum-edit/1`) matched to the markers by drum, take and recorded time (±2 ms) — a detected hit with no match is removed. The whole set is solved at once: hits struck together (15 ms) move together, nothing is sent past or onto another hit, and no slice is drawn out slower than 0.8× (what follows is moved up instead: `moved_up`). Refused, saying each `conflicts` entry, where two hits of one drum land on one line, a hit is sent onto another's attack or past it, or hits struck together are sent apart — unless a ride or a delete covers it. One undo step; `mark` names a restore point first (SESSION_HISTORY_RESTORE `mark`), else `Before DRUMS_APPLY <date time>`. `dry_run` solves and says, writing nothing. Says the check (doubled, lost, gaps), each hit's landing error over 0.5 ms (`all_landings` for every one), every slice drawn out and its rate, and the time of each stage.",
    "Drums",
    "",
);

/// What to apply.
#[derive(Facet, Default)]
pub struct ApplyArg {
    #[facet(default)]
    pub targets: HashMap<String, f64>,
    #[facet(default)]
    pub remove: Vec<String>,
    #[facet(default)]
    pub delete: Vec<String>,
    #[facet(default)]
    pub rides: HashMap<String, String>,
    /// A change list's path (`studio.drum-edit/1`).
    pub list: Option<String>,
    pub mark: Option<String>,
    pub dry_run: Option<bool>,
    /// List every hit's landing error, not only those over 0.5 ms.
    pub all_landings: Option<bool>,
}

/// One hit's landing, columnar.
#[derive(Facet, Default, Debug)]
pub struct Landings {
    pub ids: Vec<String>,
    /// Where it plays minus where it was sent, ms.
    pub off_ms: Vec<f64>,
    /// Hits sent, and of them how many landed within 0.5 ms.
    pub sent: u32,
    pub landed: u32,
    pub median_ms: Option<f64>,
    pub worst_ms: Option<f64>,
}

/// Slices drawn out or squeezed, read back from the EDIT lanes.
#[derive(Facet, Default, Debug)]
pub struct Rates {
    /// Slices whose playback rate is not 1, on any mic.
    pub stretched: u32,
    /// Of them, outside 0.8–1.25.
    pub beyond: u32,
    /// The slowest and fastest seen.
    pub slowest: Option<f64>,
    pub fastest: Option<f64>,
    /// Each stretched slice (first 60): its attack, project seconds, and
    /// its rate — columnar.
    pub at: Vec<f64>,
    pub rate: Vec<f64>,
}

/// How long each stage took, ms.
#[derive(Facet, Default, Debug)]
pub struct Stages {
    pub read: f64,
    pub solve: f64,
    pub history: f64,
    pub cut: f64,
    pub write: f64,
    pub crossfade: f64,
    pub check: f64,
    /// The time the song's state was held for writing.
    pub lock: f64,
    pub total: f64,
}

/// What a list matched.
#[derive(Facet, Default, Debug)]
pub struct Matched {
    pub matched: u32,
    /// List entries with no detected hit: `piece take recorded`.
    pub unmatched: Vec<String>,
    pub unmatched_count: u32,
    /// Detected hits with no list entry, removed.
    pub removed: u32,
}

/// What an apply did.
#[derive(Facet, Default, Debug)]
pub struct Applied {
    pub applied: bool,
    /// Why not, when refused.
    pub refused: Option<String>,
    pub hits: u32,
    pub moved: u32,
    pub removed: u32,
    pub deleted: u32,
    pub conflicts: Vec<Conflict>,
    pub moved_up: Vec<MovedUp>,
    /// Ids that play nowhere now.
    pub unknown: Vec<String>,
    /// Slices the solve draws out: their moment's first hit and rate.
    pub drawn: u32,
    pub landing: Landings,
    pub rates: Rates,
    pub check: Option<crate::hits::Check>,
    pub list: Option<Matched>,
    pub mark: Option<String>,
    pub ms: Stages,
}

fn ms(since: web_time::Instant) -> f64 {
    (since.elapsed().as_secs_f64() * 10_000.0).round() / 10.0
}

/// Every hit that plays now, with where it was recorded.
#[must_use]
pub fn strokes(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Stroke> {
    let played = crate::hits::playback(daw, project);
    let flams: HashMap<&str, &str> = played
        .iter()
        .filter_map(|p| Some((p.id.as_str(), p.mark.flam_with.as_deref()?)))
        .collect();
    crate::hits::hits(daw, project, &[])
        .into_iter()
        .filter_map(|r| {
            Some(Stroke {
                flam_with: flams.get(r.id.as_str()).map(|s| (*s).to_owned()),
                now: r.now?,
                recorded: r.time,
                peak_db: r.peak_db,
                piece: r.piece,
                id: r.id,
            })
        })
        .collect()
}

/// The edit `a` asks for, a list matched in.
fn edit_of(a: &ApplyArg, strokes: &[Stroke]) -> Result<(Edit, Option<Matched>), String> {
    let mut edit = Edit {
        targets: a.targets.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        remove: a.remove.iter().cloned().collect(),
        delete: a.delete.iter().cloned().collect(),
        rides: a
            .rides
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };
    let Some(path) = &a.list else {
        return Ok((edit, None));
    };
    let list = crate::edit_list::read(std::path::Path::new(path))?;
    let m = crate::edit_list::match_list(&list, strokes);
    for (id, to) in &m.targets {
        edit.targets.entry(id.clone()).or_insert(*to);
    }
    edit.remove.extend(m.removed.iter().cloned());
    let shown: Vec<String> = m
        .unmatched
        .iter()
        .take(40)
        .map(|h| format!("{} {} {:.4}", h.piece, h.take, h.recorded))
        .collect();
    Ok((
        edit,
        Some(Matched {
            matched: count(m.targets.len()),
            unmatched_count: count(m.unmatched.len()),
            unmatched: shown,
            removed: count(m.removed.len()),
        }),
    ))
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `DRUMS_APPLY`: see the action's description.
///
/// # Errors
/// No song, no hit markers, a list that cannot be read, no mark, or the
/// engine refusing the edit.
#[allow(clippy::too_many_lines)]
pub fn apply(a: ApplyArg) -> Result<Applied, String> {
    let began = web_time::Instant::now();
    let span = tracing::info_span!(
        "DRUMS_APPLY",
        hits = tracing::field::Empty,
        moved = tracing::field::Empty,
        removed = tracing::field::Empty,
        deleted = tracing::field::Empty,
        slices = tracing::field::Empty,
        stretched = tracing::field::Empty,
        conflicts = tracing::field::Empty,
        moved_up = tracing::field::Empty,
        lock_ms = tracing::field::Empty,
        dry_run = a.dry_run.unwrap_or(false),
        applied = tracing::field::Empty,
    );
    let _in = span.enter();
    let (daw, project) = crate::actions::song()?;
    let dry = a.dry_run.unwrap_or(false);
    let mut out = Applied::default();

    let step = tracing::info_span!("DRUMS_APPLY.read", hits = tracing::field::Empty).entered();
    let t = web_time::Instant::now();
    let strokes = strokes(daw, &project);
    if strokes.is_empty() {
        return Err("no hit plays: find the hits first (HITS_DETECT)".to_owned());
    }
    let (edit, matched) = edit_of(&a, &strokes)?;
    let before = crate::hits::playback(daw, &project);
    step.record("hits", strokes.len());
    out.ms.read = ms(t);
    drop(step);

    let step = tracing::info_span!(
        "DRUMS_APPLY.solve",
        moments = tracing::field::Empty,
        conflicts = tracing::field::Empty
    )
    .entered();
    let t = web_time::Instant::now();
    let settings = crate::drum_settings::current();
    let joins = crate::hit_place::Joins {
        lead: settings.lead_ms / 1_000.0,
        crossfade: settings.xfade_ms / 1_000.0,
    };
    let limits = Limits {
        apart: joins.lead + joins.crossfade + 0.002,
        guard: daw_edit::placement::Cut::default().guard + joins.crossfade,
        ..Limits::default()
    };
    let solution = solve(&strokes, &edit, limits);
    step.record("moments", solution.moments.len());
    step.record("conflicts", solution.conflicts.len());
    out.ms.solve = ms(t);
    drop(step);

    out.hits = count(strokes.len());
    out.moved = count(solution.moved());
    out.removed = count(
        edit.remove
            .iter()
            .filter(|id| strokes.iter().any(|s| &s.id == *id))
            .count(),
    );
    out.deleted = count(solution.moments.iter().filter(|m| m.deleted).count());
    out.conflicts.clone_from(&solution.conflicts);
    out.moved_up.clone_from(&solution.moved_up);
    out.unknown.clone_from(&solution.unknown);
    out.drawn = count(solution.drawn.len());
    out.list = matched;
    span.record("hits", out.hits);
    span.record("moved", out.moved);
    span.record("removed", out.removed);
    span.record("deleted", out.deleted);
    span.record("conflicts", solution.conflicts.len());
    span.record("moved_up", solution.moved_up.len());
    let sent = solution.sent(&edit);
    if dry || !solution.conflicts.is_empty() {
        if !solution.conflicts.is_empty() {
            out.refused = Some(format!(
                "{} conflicts: settle each (a target, a ride or a delete) and apply again",
                solution.conflicts.len()
            ));
            tracing::warn!(
                drums.conflicts = solution.conflicts.len(),
                "DRUMS_APPLY refused: conflicts"
            );
        }
        out.landing = predicted(&solution, &sent, a.all_landings.unwrap_or(false));
        out.ms.total = ms(began);
        span.record("applied", false);
        return Ok(out);
    }

    let step = tracing::info_span!("DRUMS_APPLY.history").entered();
    let t = web_time::Instant::now();
    let net = match &a.mark {
        Some(name) => {
            let daw::service::ProjectContext::Project(guid) = &project else {
                return Err("no song open".to_owned());
            };
            let want = name.clone();
            Some(crate::project_file::mark(guid, APPLY.id, move |taken| {
                unique(&want, taken)
            })?)
        }
        None => crate::safety_net::before(APPLY.id, crate::safety_net::Reach::Song)?.name(),
    };
    out.mark = net;
    out.ms.history = ms(t);
    drop(step);

    let song = crate::song::Song::current()?;
    let step = tracing::info_span!("DRUMS_APPLY.cut", ops = tracing::field::Empty).entered();
    let t = web_time::Instant::now();
    let moves = solution.moves();
    let cut_on = solution.cut_on();
    let planned = if solution.moments.iter().any(|m| m.by.abs() > NO_MOVE) {
        crate::hit_place::plan(&song, &moves, joins, (Some(&cut_on), Some(MIN_RATE)))?
    } else {
        crate::hit_place::Plan::default()
    };
    let ops = planned.ops;
    out.drawn = count(planned.drawn.len());
    step.record("ops", ops.len());
    out.ms.cut = ms(t);
    drop(step);

    let gone: BTreeSet<String> = edit
        .remove
        .iter()
        .chain(&edit.delete)
        .filter(|id| strokes.iter().any(|s| &s.id == *id))
        .cloned()
        .collect();
    let mut lock = 0.0;
    let written = song.undo_step(&undo_label(&a), |song| -> Result<(), String> {
        let step = tracing::info_span!("DRUMS_APPLY.write", ops = ops.len()).entered();
        let t = web_time::Instant::now();
        song.apply(&ops).map_err(|e| e.to_string())?;
        lock += t.elapsed().as_secs_f64();
        if !gone.is_empty() {
            crate::hits::change_markers(daw, &project, &gone, |_, _| false);
        }
        out.ms.write = ms(t);
        drop(step);
        let step = tracing::info_span!("DRUMS_APPLY.crossfade", deleted = out.deleted).entered();
        let t = web_time::Instant::now();
        let closing = close_deleted(song, &solution, (joins, limits.guard));
        let t_lock = web_time::Instant::now();
        song.apply(&closing).map_err(|e| e.to_string())?;
        lock += t_lock.elapsed().as_secs_f64();
        out.ms.crossfade = ms(t);
        drop(step);
        Ok(())
    });
    written?;
    out.ms.lock = (lock * 10_000.0).round() / 10.0;
    span.record("lock_ms", out.ms.lock);
    // The samples follow, as a quantize's do.
    let lead = joins.lead;
    let mut shifts: Vec<(f64, f64)> = solution
        .moments
        .iter()
        .map(|m| (m.now - lead, m.by))
        .collect();
    shifts.sort_by(|x, y| x.0.total_cmp(&y.0));
    let goes_to = move |t: f64| {
        let i = shifts.partition_point(|s| s.0 <= t);
        t + i
            .checked_sub(1)
            .and_then(|i| shifts.get(i))
            .map_or(0.0, |s| s.1)
    };
    if let Err(why) =
        crate::hits_midi::sync(daw, &project, crate::hits_midi::Mode::Follow, &goes_to)
    {
        tracing::warn!(why, "drums: the samples could not follow the apply");
    }
    crate::song_model::request_resync_items();
    crate::hit_overlay::refresh();

    let step = tracing::info_span!(
        "DRUMS_APPLY.check",
        slices = tracing::field::Empty,
        stretched = tracing::field::Empty
    )
    .entered();
    let t = web_time::Instant::now();
    let before: Vec<crate::hits::Played> = before
        .into_iter()
        .filter(|p| !gone.contains(&p.id))
        .collect();
    let targets: Vec<f64> = sent.iter().map(|(_, _, to)| *to).collect();
    let whole = (
        solution
            .moments
            .first()
            .map_or(0.0, |m| m.now.min(m.lands()))
            - 1.0,
        solution
            .moments
            .last()
            .map_or(0.0, |m| m.now.max(m.lands()))
            + 1.0,
    );
    out.check = Some(crate::hits::check(daw, &project, &before, &targets, whole));
    out.landing = landed(
        daw,
        &project,
        &solution,
        &sent,
        a.all_landings.unwrap_or(false),
    );
    let (slices, rates) = rates_of(&song, joins.lead);
    out.rates = rates;
    step.record("slices", slices);
    step.record("stretched", out.rates.stretched);
    span.record("slices", slices);
    span.record("stretched", out.rates.stretched);
    out.ms.check = ms(t);
    drop(step);
    out.applied = true;
    span.record("applied", true);
    out.ms.total = ms(began);
    crate::actions::list().changed("items", "drums applied");
    Ok(out)
}

/// `name`, or with ` 2`, ` 3`, … after it when a mark is already called
/// that.
#[must_use]
pub fn unique(name: &str, taken: &[String]) -> String {
    std::iter::once(name.to_owned())
        .chain((2_u32..).map(|n| format!("{name} {n}")))
        .find(|n| !taken.iter().any(|t| t == n))
        .unwrap_or_else(|| name.to_owned())
}

fn undo_label(a: &ApplyArg) -> String {
    a.mark.as_ref().map_or_else(
        || "Apply a drum edit".to_owned(),
        |m| format!("Apply a drum edit: {m}"),
    )
}

/// The landings the solve predicts (a dry run, or a refusal).
fn predicted(solution: &Solution, sent: &[(usize, f64, f64)], all: bool) -> Landings {
    landings(
        sent.iter()
            .filter_map(|(k, lands, to)| Some((solution.strokes.get(*k)?.id.clone(), lands - to))),
        all,
    )
}

/// Where each hit sent plays now, against where it was sent.
fn landed(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    solution: &Solution,
    sent: &[(usize, f64, f64)],
    all: bool,
) -> Landings {
    let mut now: HashMap<&str, Vec<f64>> = HashMap::new();
    let after = crate::hits::playback(daw, project);
    for p in after.iter() {
        now.entry(p.id.as_str()).or_default().push(p.time);
    }
    landings(
        sent.iter().filter_map(|(k, _, to)| {
            let id = &solution.strokes.get(*k)?.id;
            let plays = now
                .get(id.as_str())?
                .iter()
                .copied()
                .min_by(|a, b| (a - to).abs().total_cmp(&(b - to).abs()))?;
            Some((id.clone(), plays - to))
        }),
        all,
    )
}

fn landings(offs: impl Iterator<Item = (String, f64)>, all: bool) -> Landings {
    let mut out = Landings::default();
    let mut abs: Vec<f64> = Vec::new();
    for (id, off) in offs {
        let off_ms = (off * 10_000.0).round() / 10.0;
        out.sent = out.sent.saturating_add(1);
        abs.push(off_ms.abs());
        if off_ms.abs() <= LANDED_MS {
            out.landed = out.landed.saturating_add(1);
        }
        if all || (off_ms.abs() > LANDED_MS && out.ids.len() < LANDINGS) {
            out.ids.push(id);
            out.off_ms.push(off_ms);
        }
    }
    abs.sort_by(f64::total_cmp);
    out.median_ms = abs.get(abs.len() / 2).copied();
    out.worst_ms = abs.last().copied();
    out
}

/// What cuts each deleted moment's slice out of every mic's EDIT lane:
/// the slice gone, the one before drawn out to meet the next.
fn close_deleted(
    song: &crate::song::Song,
    solution: &Solution,
    (joins, guard): (crate::hit_place::Joins, f64),
) -> Vec<ItemOp> {
    // Each deleted moment where it lands, and the earliest the slice after
    // it may start: its own recording back to the deleted attack and a
    // guard past it.
    let deleted: Vec<Gone> = solution
        .moments
        .iter()
        .enumerate()
        .filter(|(_, m)| m.deleted)
        .map(|(i, m)| {
            let after = solution.moments.iter().skip(i + 1).find(|n| !n.deleted);
            Gone {
                at: m.lands(),
                earliest: after.map_or(f64::NEG_INFINITY, |n| {
                    m.recorded + (n.lands() - n.recorded) + guard
                }),
            }
        })
        .collect();
    if deleted.is_empty() {
        return Vec::new();
    }
    let mics = crate::hit_place::mics(song);
    let (lo, hi) = (
        deleted.first().map_or(0.0, |g| g.at) - 10.0,
        deleted.last().map_or(0.0, |g| g.at) + 10.0,
    );
    let lanes: Vec<Vec<crate::hit_place::Slice>> = song
        .read(|p| {
            mics.iter()
                .map(|m| crate::hit_place::lane_of(p, &m.track, m.edit, (lo, hi)))
                .collect()
        })
        .unwrap_or_default();
    let mut ops = Vec::new();
    for lane in &lanes {
        for gone in &deleted {
            close_one(lane, *gone, joins, &mut ops);
        }
    }
    ops
}

/// A deleted moment: where it lands, and the earliest the slice after it
/// may start.
#[derive(Clone, Copy, Debug)]
struct Gone {
    at: f64,
    earliest: f64,
}

/// One mic's slice at `gone.at` (an attack) cut out, the one before
/// drawn out to meet the slice after — no slower than [`MIN_RATE`], the
/// slice after starting earlier for the rest, as [`limit_draws`] does.
fn close_one(
    lane: &[crate::hit_place::Slice],
    gone: Gone,
    joins: crate::hit_place::Joins,
    ops: &mut Vec<ItemOp>,
) {
    let Some(k) = lane
        .iter()
        .position(|s| (s.start() + joins.lead - gone.at).abs() <= 0.002)
    else {
        return;
    };
    let (Some(cut), Some(prev)) = (lane.get(k), k.checked_sub(1).and_then(|p| lane.get(p))) else {
        return;
    };
    // Trimmed to just before the deleted attack in its own recording,
    // then drawn out: never through the stroke that goes.
    let natural = (gone.at - joins.lead - prev.start()).max(0.001);
    let most = prev.start() + farthest(natural);
    let mut to = cut.end();
    if let Some(next) = lane.get(k + 1) {
        let mut start = next.start();
        let want = most.max(gone.earliest);
        let offset = next.item.start_offset.as_seconds() - (start - want);
        if want < start - 1e-6 && offset >= 0.0 && next.markers.len() < 2 {
            start = want;
            let id = ItemId::from(next.item.guid.as_str());
            ops.push(ItemOp::Place {
                item: id.clone(),
                at: start,
                length: next.end() - start,
                offset,
            });
            ops.push(ItemOp::Fades {
                item: id,
                fade_in: joins.crossfade,
                fade_out: next.item.fade_out_length.as_seconds(),
            });
        }
        to = start + joins.crossfade;
    }
    let mut shape = prev.item.clone();
    shape.length = daw_proto::Duration::from_seconds(natural);
    let markers = crate::stretch::draw_out_to(&mut shape, to, MIN_RATE)
        .or_else(|| crate::stretch::draw_out_markers(&mut shape, to))
        .unwrap_or_default();
    let item = ItemId::from(prev.item.guid.as_str());
    ops.push(ItemOp::Delete {
        item: ItemId::from(cut.item.guid.as_str()),
    });
    ops.push(ItemOp::Place {
        item: item.clone(),
        at: prev.start(),
        length: shape.length.as_seconds(),
        offset: prev.item.start_offset.as_seconds(),
    });
    ops.push(ItemOp::Fades {
        item: item.clone(),
        fade_in: prev.item.fade_in_length.as_seconds(),
        fade_out: joins.crossfade,
    });
    if !markers.is_empty() {
        ops.push(ItemOp::Stretch {
            item,
            markers: markers
                .into_iter()
                .map(|(t, s)| daw_proto::StretchMarker::new(t, s))
                .collect(),
        });
    }
}

/// Every EDIT slice's playback rate, read back off the lanes: how many
/// slices there are, and those not at 1.
fn rates_of(song: &crate::song::Song, lead: f64) -> (usize, Rates) {
    let mics = crate::hit_place::mics(song);
    let segments: Vec<(f64, f64, f64)> = song
        .read(|p| {
            let mut all = Vec::new();
            for m in &mics {
                for s in crate::hit_place::lane_of(
                    p,
                    &m.track,
                    m.edit,
                    (f64::NEG_INFINITY, f64::INFINITY),
                ) {
                    let (slow, fast) = segment_rates(&s.markers);
                    all.push((s.start() + lead, slow, fast));
                }
            }
            all
        })
        .unwrap_or_default();
    let mut out = Rates::default();
    let slices = segments.len();
    for (at, slow, fast) in segments {
        if (slow - 1.0).abs() < 1e-3 && (fast - 1.0).abs() < 1e-3 {
            continue;
        }
        out.stretched = out.stretched.saturating_add(1);
        if slow < MIN_RATE - 1e-3 || fast > MAX_RATE + 1e-3 {
            out.beyond = out.beyond.saturating_add(1);
        }
        out.slowest = Some(out.slowest.map_or(slow, |s: f64| s.min(slow)));
        out.fastest = Some(out.fastest.map_or(fast, |f: f64| f.max(fast)));
        if out.at.len() < LANDINGS {
            out.at.push((at * 10_000.0).round() / 10_000.0);
            let shown = if (slow - 1.0).abs() > (fast - 1.0).abs() {
                slow
            } else {
                fast
            };
            out.rate.push((shown * 1_000.0).round() / 1_000.0);
        }
    }
    (slices, out)
}

/// The slowest and fastest of a slice's stretch segments (1, 1 unstretched).
#[must_use]
pub fn segment_rates(markers: &[daw_proto::StretchMarker]) -> (f64, f64) {
    let mut out = (1.0_f64, 1.0_f64);
    for w in markers.windows(2) {
        let [a, b] = w else { continue };
        let dt = b.position - a.position;
        if dt <= 1e-9 {
            continue;
        }
        let r = (b.source_position - a.source_position) / dt;
        out = (out.0.min(r), out.1.max(r));
    }
    out
}

/// Register `DRUMS_APPLY`, the change lists, the bench and the edits.
///
/// # Errors
/// When one is already registered.
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&APPLY, Effect::Mutating, apply)?;
    crate::edit_list::register(list)?;
    crate::hit_ops::register(list)?;
    crate::drum_bench::register(list)?;
    Ok(())
}

#[cfg(test)]
mod tests;
