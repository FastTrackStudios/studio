//! What the comp holds to: the kit's **main take** — the one take the
//! song is played from, left only where it has to be — and stretches
//! **kept** as they are, a fill whose take was chosen by ear, held
//! against everything that would choose again.
//!
//! With a main take, T and Shift+T hold the takes up to it and rank it
//! first, and COMP_BEST leaves it only where another take alike to it
//! plays clearly better (`comp_song`).
//!
//! A kept stretch names the take it is comped from. T and Shift+T over
//! it hold the takes up to that take (and rank it first); COMP_SEAMS
//! never moves its seams into it; COMP_BEST leaves it out. Kept on the
//! kit's folder with the song, so an undo takes a keep back with the rest.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{ProjectContext, TrackRef, Tracks};
use facet::Facet;

/// Where the kept stretches are written: on the kit's folder.
const KEY: &str = "fts.comp_kept";
/// Where the main take is written: on the kit's folder.
const MAIN_KEY: &str = "fts.comp_main";

/// The kit's main take, its recording pass, if one is set.
#[must_use]
pub fn main_take(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Option<String> {
    let folder = crate::track_regions::kit_folder(daw, project)?;
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(folder))
        .into_iter()
        .find(|e| e.key == MAIN_KEY)
        .map(|e| e.value)
        .filter(|v| !v.is_empty())
}

fn set_main_take(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    pass: &str,
) -> Result<(), String> {
    let folder = crate::track_regions::kit_folder(daw, project).ok_or("no kit folder")?;
    Tracks::set_ext_state(daw, project.clone(), TrackRef::Guid(folder), MAIN_KEY, pass)
        .map_err(|e| e.to_string())
}

/// A stretch kept as comped, and the take it is comped from.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct Kept {
    /// Song seconds.
    pub from: f64,
    pub to: f64,
    /// The take: its recording pass.
    pub pass: String,
}

/// The song's kept stretches, in time order.
#[must_use]
pub fn kept(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Kept> {
    let Some(folder) = crate::track_regions::kit_folder(daw, project) else {
        return Vec::new();
    };
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(folder))
        .into_iter()
        .find(|e| e.key == KEY)
        .and_then(|e| facet_json::from_str::<Vec<Kept>>(&e.value).ok())
        .unwrap_or_default()
}

fn write(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    all: &[Kept],
) -> Result<(), String> {
    let folder = crate::track_regions::kit_folder(daw, project).ok_or("no kit folder")?;
    let value = facet_json::to_string(&all.to_vec()).map_err(|e| e.to_string())?;
    Tracks::set_ext_state(daw, project.clone(), TrackRef::Guid(folder), KEY, &value)
        .map_err(|e| e.to_string())
}

/// The kept stretch that holds most of `from..to` — more than half of it
/// — if one does.
#[must_use]
pub fn holding(all: &[Kept], (from, to): (f64, f64)) -> Option<&Kept> {
    let half = (to - from) / 2.0;
    all.iter()
        .map(|k| (k, k.to.min(to) - k.from.max(from)))
        .filter(|(_, shared)| *shared > half)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
}

/// `all` with `keep` in it: the kept stretches it overlaps taken out.
#[must_use]
pub fn with(all: &[Kept], keep: Kept) -> Vec<Kept> {
    let mut out: Vec<Kept> = all
        .iter()
        .filter(|k| k.to <= keep.from || k.from >= keep.to)
        .cloned()
        .collect();
    out.push(keep);
    out.sort_by(|a, b| a.from.total_cmp(&b.from));
    out
}

/// `all` without the kept stretches `from..to` overlaps.
#[must_use]
pub fn without(all: &[Kept], (from, to): (f64, f64)) -> Vec<Kept> {
    all.iter()
        .filter(|k| k.to <= from || k.from >= to)
        .cloned()
        .collect()
}

/// `spans` with the kept stretches cut out of them: what is left to
/// choose a take for.
#[must_use]
pub fn outside(spans: &[(f64, f64)], all: &[Kept]) -> Vec<(f64, f64)> {
    spans
        .iter()
        .flat_map(|&(from, to)| {
            let mut left = vec![(from, to)];
            for k in all {
                left = left
                    .into_iter()
                    .flat_map(|(a, b)| {
                        if k.to <= a || k.from >= b {
                            vec![(a, b)]
                        } else {
                            [(a, k.from), (k.to, b)]
                                .into_iter()
                                .filter(|(x, y)| y - x > crate::comp::GRAIN)
                                .collect()
                        }
                    })
                    .collect();
            }
            left
        })
        .collect()
}

static KEEP: ActionMeta = crate::actions::meta(
    "COMP_KEEP",
    "comp_keep",
    "Keep the take comped here",
    "Keep a stretch of the kit's comp as it is — a fill whose take was chosen by ear: the take the comp plays over it now is remembered with the stretch. T / Shift+T over it hold the takes up to that take and rank it first; COMP_SEAMS never moves its seams into it; COMP_BEST leaves it out. The stretch is the selection (or `from`/`to`); `release` true lets it go again instead; pressed on a kept stretch with nothing said, it is let go (Ctrl+Shift+T toggles). One undo step. Says every kept stretch.",
    "Comp",
    "",
);

/// Over when, and whether to let go.
#[derive(Facet)]
struct KeepArg {
    #[facet(flatten, default)]
    on: crate::edit_actions::On,
    release: Option<bool>,
    /// Let go what is kept there, else keep it (the key's toggle).
    toggle: Option<bool>,
}

/// What is kept now.
#[derive(Facet)]
struct KeepOut {
    kept: Vec<Kept>,
    /// What it did: `kept` or `released`, per stretch.
    did: Vec<String>,
}

pub(crate) fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&KEEP, Effect::Mutating, |a: KeepArg| {
        let (daw, project) = crate::actions::song()?;
        let stretches = crate::edit_actions::stretches(daw, &project, &a.on)?;
        let label = "Keep the take comped here";
        daw::service::Projects::begin_undo_block(daw, project.clone(), label);
        let done = stretches
            .iter()
            .map(|&span| {
                let all = kept(daw, &project);
                let release = a.release.unwrap_or(false)
                    || (a.toggle.unwrap_or(false) && holding(&all, span).is_some());
                if release {
                    write(daw, &project, &without(&all, span))?;
                    return Ok(format!("released {:.3}-{:.3}", span.0, span.1));
                }
                let versions = crate::comp_rank::versions_over(daw, &project, span);
                let pass = crate::comp_rank::current(daw, &project, &versions, span)
                    .ok_or("no take is comped there")?;
                let keep = Kept {
                    from: span.0,
                    to: span.1,
                    pass: pass.clone(),
                };
                write(daw, &project, &with(&all, keep))?;
                Ok(format!("kept {:.3}-{:.3} from {pass}", span.0, span.1))
            })
            .collect::<Result<Vec<String>, String>>();
        daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
        let did = done?;
        crate::comp_rank::forget();
        crate::hit_overlay::refresh();
        Ok(KeepOut {
            kept: kept(daw, &project),
            did,
        })
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(from: f64, to: f64, pass: &str) -> Kept {
        Kept {
            from,
            to,
            pass: pass.to_owned(),
        }
    }

    #[test]
    fn the_kept_stretch_holding_most_of_a_span() {
        let all = [k(12.75, 13.5, "a"), k(20.0, 21.0, "b")];
        assert_eq!(
            holding(&all, (12.8, 13.4)).map(|k| k.pass.as_str()),
            Some("a")
        );
        assert_eq!(
            holding(&all, (12.0, 13.0)),
            None,
            "a third of it is not most"
        );
        assert_eq!(holding(&all, (15.0, 16.0)), None);
    }

    #[test]
    fn keeping_again_replaces_what_it_overlaps() {
        let all = vec![k(1.0, 2.0, "a"), k(5.0, 6.0, "b")];
        let now = with(&all, k(1.5, 2.5, "c"));
        assert_eq!(now, vec![k(1.5, 2.5, "c"), k(5.0, 6.0, "b")]);
        assert_eq!(without(&now, (5.5, 5.6)), vec![k(1.5, 2.5, "c")]);
    }

    #[test]
    fn the_kept_stretches_are_cut_out_of_what_is_chosen_again() {
        let all = [k(12.75, 13.5, "a")];
        assert_eq!(
            outside(&[(10.0, 16.0)], &all),
            vec![(10.0, 12.75), (13.5, 16.0)]
        );
        assert_eq!(outside(&[(13.0, 13.4)], &all), Vec::<(f64, f64)>::new());
        assert_eq!(outside(&[(1.0, 2.0)], &all), vec![(1.0, 2.0)]);
    }
}

static MAIN: ActionMeta = crate::actions::meta(
    "COMP_MAIN",
    "comp_main",
    "Comp the kit from its main take",
    "Make `take` (its pass, colour — `Teal` — or lane name, as COMP_CANDIDATES names takes) the kit's main take and comp it back in wherever the comp plays another — except the stretches kept (COMP_KEEP) and where it has no audio — every mic from its own lane, the EDIT lane there the new comp as recorded (quantize it again after), one undo step. From then on T / Shift+T hold the takes up to it and rank it first, and COMP_BEST leaves it only where a take alike to it plays clearly better. `take` left out: the main take set, else none. `dry_run` says the stretches it would comp. Says each stretch and what played there.",
    "Comp",
    "",
);

#[derive(Facet)]
struct MainArg {
    take: Option<String>,
    dry_run: Option<bool>,
}

/// A stretch comped back to the main take.
#[derive(Facet)]
struct Restored {
    from: f64,
    to: f64,
    /// What played there.
    was: String,
}

#[derive(Facet)]
struct MainOut {
    main: String,
    restored: Vec<Restored>,
    /// Stretches left on another take: kept, or the main take silent there.
    left: Vec<String>,
}

/// The stretches of the comp playing another take than `main`, each with
/// what plays there, the kept ones cut out.
fn off_main(
    comps: &[crate::hit_overlay::CompSpan],
    main: &str,
    kept: &[Kept],
) -> Vec<(f64, f64, String)> {
    comps
        .iter()
        .filter(|c| crate::take_colors::stamp(&c.take).as_deref() != Some(main))
        .flat_map(|c| {
            outside(&[(c.start, c.end)], kept)
                .into_iter()
                .map(|(a, b)| (a, b, c.take.clone()))
        })
        .collect()
}

pub(crate) fn register_main(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&MAIN, Effect::Mutating, |a: MainArg| {
        let (daw, project) = crate::actions::song()?;
        let song_span = (f64::NEG_INFINITY, f64::INFINITY);
        let versions = crate::comp_rank::versions_over(daw, &project, song_span);
        let pass = match a.take.as_deref() {
            Some(name) => crate::comp_rank::pass_named(&project, &versions, name)
                .ok_or_else(|| format!("no take called {name}"))?,
            None => main_take(daw, &project).ok_or("no main take set: say which")?,
        };
        let main = versions
            .iter()
            .find(|v| v.pass == pass)
            .ok_or("the main take has no recordings on the kit")?;
        let (kit, _) = crate::hits::kit(daw, &project);
        let comps = crate::hits::comps(daw, &project, &kit);
        let stretches = off_main(&comps, &pass, &kept(daw, &project));
        let song = crate::hit_index::song_of(&project).ok_or("no song")?;
        // The main take comps back in where it has audio and plays: a
        // stretch where only another take plays (an ending another take
        // holds) stays on that take.
        let plays = |v: &crate::hits::TakeVersion, (from, to): (f64, f64)| {
            v.hits.iter().any(|(_, t)| *t >= from && *t < to)
        };
        let others_play =
            |span: (f64, f64)| versions.iter().any(|v| v.pass != pass && plays(v, span));
        let (playable, silent): (Vec<_>, Vec<_>) =
            stretches.into_iter().partition(|(from, to, _)| {
                let span = (*from, *to);
                crate::comp_song::Audio::read(&song, std::slice::from_ref(main), span)
                    .covers(main, span)
                    && (plays(main, span) || !others_play(span))
            });
        let mut left: Vec<String> = kept(daw, &project)
            .iter()
            .map(|k| format!("kept {:.3}-{:.3} on {}", k.from, k.to, k.pass))
            .collect();
        left.extend(silent.iter().map(|(f, t, was)| {
            format!("{f:.3}-{t:.3} on {was}: the main take has no audio or plays nothing there")
        }));
        if !a.dry_run.unwrap_or(false) {
            let label = "Comp the kit from its main take";
            daw::service::Projects::begin_undo_block(daw, project.clone(), label);
            let done = set_main_take(daw, &project, &pass).and_then(|()| {
                playable.iter().try_for_each(|&(from, to, _)| {
                    crate::comp::swipe_lanes(daw, &project, &main.lanes, (from, to), label)?;
                    crate::comp::silence(daw, &project, &main.unrecorded(&versions), (from, to))
                        .map_err(|e| e.to_string())?;
                    let tracks: Vec<String> = main.lanes.iter().map(|(t, _)| t.clone()).collect();
                    crate::comp::edit_from_comp(daw, &project, &tracks, (from, to)).map(|_| ())
                })
            });
            daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
            done?;
            crate::comp_rank::forget();
            crate::studio::request_resync_items();
            crate::hit_overlay::refresh();
        }
        Ok(MainOut {
            main: pass,
            restored: playable
                .into_iter()
                .map(|(from, to, was)| Restored { from, to, was })
                .collect(),
            left,
        })
    })?;
    Ok(())
}
