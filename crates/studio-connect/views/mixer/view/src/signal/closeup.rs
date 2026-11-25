//! A Signal block, close up: its parameters on the faders every FTS
//! editor uses (`fts_audio_ui::controls::HSlider`), its meters, its
//! switch, and the structural edits — remove, move, add a block after it.
//!
//! What `SESSION_CLOSEUP {track, block}` shows ([`crate::closeup`]). A
//! value goes out through `FX_SET` on its `clap_id`; everything else
//! through `SIGNAL_RUN`. The chain is followed while it is open, and the
//! meters read ten times a second.

use std::sync::{Arc, Mutex};

use dioxus::prelude::*;
use fts_audio_ui::ParamHandle;
use fts_audio_ui::controls::HSlider;

use super::chain::{BlockId, Param, PhaseId};
use super::source::{Edit, ParamValue};
use super::view::Inserts;

/// The mixer's and the close-ups' one view of every Signal FX.
pub type Shared = Arc<Mutex<Inserts>>;

/// `inserts`, locked.
pub fn lock(inserts: &Shared) -> std::sync::MutexGuard<'_, Inserts> {
    inserts
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The kinds a block can be added as, in the order the close-up offers them.
pub const KINDS: [&str; 7] = ["eq", "comp", "gate", "saturate", "reverb", "delay", "nam"];

/// What one block's close-up shows, read from the chain under the lock.
#[derive(Clone, PartialEq)]
struct Shown {
    name: String,
    kind: String,
    algorithm: Option<String>,
    phase: PhaseId,
    phase_name: String,
    bypass: bool,
    status: String,
    trouble: bool,
    key: String,
    params: Vec<Param>,
    meters: Vec<(String, String, Option<f32>)>,
    /// The blocks of its phase, in order: what a move goes before or after.
    neighbours: Vec<BlockId>,
}

fn shown(inserts: &Inserts, track: &str, block: &str) -> Option<Shown> {
    let strip = inserts.strip(track)?;
    let (phase, b) = strip.chain.block(block)?;
    Some(Shown {
        name: b.name.clone(),
        kind: b.kind.clone(),
        algorithm: b.algorithm.clone(),
        phase: phase.id.clone(),
        phase_name: phase.name.clone(),
        bypass: b.bypass,
        status: b.status.label().to_owned(),
        trouble: b.status.is_trouble(),
        key: super::paint::second_line(b).0,
        params: b.params.clone(),
        meters: b
            .meters
            .iter()
            .map(|m| (m.name.clone(), m.unit.clone(), strip.meter(m.index)))
            .collect(),
        neighbours: phase.blocks.iter().map(|b| b.id.clone()).collect(),
    })
}

/// The block one place earlier (`-1`) or later (`+1`) in its phase is
/// after: the `after` a move takes.
fn moved_after(neighbours: &[BlockId], block: &BlockId, step: isize) -> Option<Option<BlockId>> {
    let at = neighbours.iter().position(|b| b == block)?;
    let to = at.checked_add_signed(step)?;
    if to >= neighbours.len() {
        return None;
    }
    let others: Vec<&BlockId> = neighbours.iter().filter(|b| *b != block).collect();
    Some(
        to.checked_sub(1)
            .and_then(|i| others.get(i))
            .map(|b| (*b).clone()),
    )
}

/// A parameter as the faders drive it: read and written through
/// `inserts`, so the strip and this close-up show one value.
fn handle(inserts: &Shared, track: &str, block: &BlockId, param: &Param) -> ParamHandle {
    let read = |inserts: &Shared, track: &str, block: &BlockId, id: &str| {
        lock(inserts)
            .strip(track)
            .and_then(|s| s.chain.block(block.as_str()))
            .and_then(|(_, b)| b.params.iter().find(|p| p.id == id).cloned())
    };
    let (i1, t1, b1, p1) = (
        inserts.clone(),
        track.to_owned(),
        block.clone(),
        param.id.clone(),
    );
    let (i2, t2, b2, p2) = (
        inserts.clone(),
        track.to_owned(),
        block.clone(),
        param.id.clone(),
    );
    let (i3, t3, b3, p3) = (
        inserts.clone(),
        track.to_owned(),
        block.clone(),
        param.id.clone(),
    );
    let name = param.name.clone();
    let choices = param.choices.clone();
    let (min, max) = (param.min, param.max);
    let range = max - min;
    ParamHandle::new(
        move || {
            read(&i1, &t1, &b1, &p1).map_or(0.0, |p| crate::num::narrow(p.normalized))
        },
        || {},
        move |v| {
            let value = ParamValue::Normalized(f64::from(v));
            let set = lock(&i2).set_param(&t2, &b2, &p2, &value);
            if let Err(e) = set {
                tracing::warn!(signal.block = b2.as_str(), signal.param = p2.as_str(), error = %e, "a Signal parameter was refused");
            }
        },
        || {},
        move || read(&i3, &t3, &b3, &p3).map(|p| p.text).unwrap_or_default(),
        move || name.clone(),
        move |text| {
            let text = text.trim();
            if let Some(at) = choices.iter().position(|c| c.eq_ignore_ascii_case(text)) {
                let last = choices.len().saturating_sub(1).max(1);
                return Some(crate::num::narrow(crate::num::coord(at) / crate::num::coord(last)));
            }
            let v = crate::tone_read::number(text)?;
            (range > 0.0).then(|| crate::num::narrow(((v - min) / range).clamp(0.0, 1.0)))
        },
    )
    .with_default(crate::num::narrow(if range > 0.0 {
        ((param.default - min) / range).clamp(0.0, 1.0)
    } else {
        0.0
    }))
    .with_unit(&param.unit)
    .with_step_count((!param.choices.is_empty()).then_some(param.choices.len()))
    .with_bipolar(min < 0.0 && max > 0.0 && param.unit == "dB")
}

/// Run `edit` on `track`'s Signal FX, saying a refusal once.
fn run(inserts: &Shared, track: &str, edit: &Edit) {
    let ran = lock(inserts).run(track, edit);
    if let Err(e) = ran {
        tracing::warn!(signal.action = edit.action().0, error = %e, "a Signal edit was refused");
    }
}

const BUTTON: &str = "height:28px; padding:0 10px; border-radius:6px; border:1px solid #2a2c31; \
     background:#1c1e22; color:#e5e7eb; font-family:inherit; font-size:12px; \
     display:flex; align-items:center; justify-content:center; white-space:nowrap;";

/// One Signal block, the whole close-up layer.
#[component]
pub fn SignalBlockCloseup(track: String, block: String) -> Element {
    let closeups: crate::closeup::Closeups = use_context();
    let inserts = closeups.signal;
    let mut tick = use_signal(|| 0_u64);
    {
        let inserts = inserts.clone();
        use_future(move || {
            let inserts = inserts.clone();
            async move {
                loop {
                    crate::pause(std::time::Duration::from_millis(100)).await;
                    let mut held = lock(&inserts);
                    let followed = held.follow();
                    let read = held.read_meters(web_time::Instant::now());
                    drop(held);
                    if followed.changed() || read {
                        tick += 1;
                    }
                }
            }
        });
    }
    let _ = tick();
    let Some(shown) = shown(&lock(&inserts), &track, &block) else {
        return rsx! {
            div {
                style: "padding:24px; color:#9ca3af; font-size:14px;",
                "This block is no longer in the track's Signal chain."
            }
        };
    };
    let id = BlockId(block);
    // The family's algorithms, asked once per block kind.
    let algorithms = {
        let (inserts, track, kind) = (inserts.clone(), track.clone(), shown.kind.clone());
        use_memo(use_reactive!(|(track, kind)| {
            lock(&inserts).algorithms(&track, &kind).unwrap_or_default()
        }))
    };
    let header = header(&inserts, &track, &id, &shown, tick);
    let picker = algorithm_row(&inserts, &track, &id, &shown, &algorithms.read(), tick);
    let sliders = shown.params.iter().map(|param| {
        let exposed = param.clap_id.is_some();
        let handle = handle(&inserts, &track, &id, param);
        let key = format!("{}-{}", param.id, param.clap_id.as_deref().unwrap_or("-"));
        rsx! {
            div {
                key: "{key}",
                style: "display:flex; flex-direction:column; gap:2px; min-width:0;",
                HSlider { handle, disabled: !exposed, height: 26.0 }
                if !exposed {
                    div { style: "font-size:10px; color:#6b7280;", "not exposed to the host" }
                }
            }
        }
    });
    let meters = shown.meters.iter().map(|(name, unit, value)| {
        let words = value.map_or_else(|| "—".to_owned(), |v| format!("{v:.1} {unit}"));
        rsx! {
            div {
                key: "{name}",
                style: "display:flex; flex-direction:column; gap:2px; padding:8px 12px; \
                        border-radius:8px; background:#17181b; border:1px solid #2a2c31; min-width:84px;",
                span { style: "font-size:10px; color:#9ca3af; letter-spacing:0.04em;", "{name}" }
                span { style: "font-size:16px; font-weight:650; color:#e5e7eb;", "{words}" }
            }
        }
    });
    rsx! {
        fts_audio_ui::drag::DragProvider { fill: true,
            div {
                style: "width:100%; height:100%; display:flex; flex-direction:column; gap:14px; \
                        padding:16px; overflow-y:scroll; box-sizing:border-box;",
                {header}
                {picker}
                if !shown.meters.is_empty() {
                    div { style: "display:flex; flex-direction:row; gap:8px; flex-wrap:wrap;", {meters} }
                }
                div {
                    style: "display:grid; grid-template-columns:repeat(auto-fill, minmax(220px, 1fr)); \
                            gap:12px 18px; min-width:0;",
                    {sliders}
                }
                {add_row(&inserts, &track, &shown, &id, tick)}
            }
        }
    }
}

/// The block's name, where it sits, its trouble or key, its switch and
/// its moves.
fn header(
    inserts: &Shared,
    track: &str,
    id: &BlockId,
    shown: &Shown,
    mut tick: Signal<u64>,
) -> Element {
    let status_ink = if shown.trouble { "#ef4444" } else { "#60a5fa" };
    let line = if shown.trouble {
        shown.status.clone()
    } else {
        shown.key.clone()
    };
    let bypass_edit = Edit::BypassBlock {
        block: id.clone(),
        bypass: !shown.bypass,
    };
    let remove_edit = Edit::Remove { block: id.clone() };
    let move_edit = |step| {
        moved_after(&shown.neighbours, id, step).map(|after| Edit::Move {
            block: id.clone(),
            phase: shown.phase.clone(),
            after,
        })
    };
    let (up, down) = (move_edit(-1), move_edit(1));
    let button = |label: &'static str, edit: Option<Edit>| {
        let inserts = inserts.clone();
        let track = track.to_owned();
        let opacity = if edit.is_none() { "0.4" } else { "1" };
        rsx! {
            button {
                style: "{BUTTON} opacity:{opacity};",
                onclick: move |_| {
                    if let Some(edit) = &edit {
                        run(&inserts, &track, edit);
                        tick += 1;
                    }
                },
                "{label}"
            }
        }
    };
    let on_label = if shown.bypass { "Bypassed" } else { "On" };
    rsx! {
        div {
            style: "display:flex; flex-direction:row; align-items:center; gap:10px; flex-wrap:wrap;",
            div {
                style: "display:flex; flex-direction:column; gap:2px; min-width:0; flex:1;",
                span { style: "font-size:18px; font-weight:650;", "{shown.name}" }
                span {
                    style: "font-size:12px; color:#9ca3af;",
                    "{shown.kind} · {shown.phase_name}"
                }
                if !line.is_empty() {
                    span { style: "font-size:12px; color:{status_ink};", "{line}" }
                }
            }
            {button(on_label, Some(bypass_edit))}
            {button("Move up", up)}
            {button("Move down", down)}
            {button("Remove", Some(remove_edit))}
        }
    }
}

/// The family's algorithms, the one it runs lit: each sets it
/// (`SIGNAL_SET {block, algorithm}`). Nothing when the family has none.
fn algorithm_row(
    inserts: &Shared,
    track: &str,
    id: &BlockId,
    shown: &Shown,
    algorithms: &[super::source::Algorithm],
    mut tick: Signal<u64>,
) -> Element {
    if algorithms.is_empty() {
        return rsx! {};
    }
    let buttons = algorithms.iter().map(|algorithm| {
        let inserts = inserts.clone();
        let track = track.to_owned();
        let on = shown.algorithm.as_deref() == Some(algorithm.id.as_str());
        let look = if on {
            "border-color:#3b82f6; background:#1e3a5f; font-weight:650;"
        } else {
            ""
        };
        let edit = Edit::Algorithm {
            block: id.clone(),
            algorithm: algorithm.id.clone(),
        };
        rsx! {
            button {
                key: "{algorithm.id}",
                style: "{BUTTON} {look}",
                onclick: move |_| {
                    run(&inserts, &track, &edit);
                    tick += 1;
                },
                "{algorithm.name}"
            }
        }
    });
    rsx! {
        div {
            style: "display:flex; flex-direction:row; align-items:center; gap:6px; flex-wrap:wrap;",
            span { style: "font-size:12px; color:#9ca3af; margin-right:4px;", "Algorithm:" }
            {buttons}
        }
    }
}

/// A row of kinds: each adds a block of it after this one.
fn add_row(
    inserts: &Shared,
    track: &str,
    shown: &Shown,
    id: &BlockId,
    mut tick: Signal<u64>,
) -> Element {
    let kinds = KINDS.iter().map(|kind| {
        let inserts = inserts.clone();
        let track = track.to_owned();
        let edit = Edit::Add {
            phase: shown.phase.clone(),
            kind: (*kind).to_owned(),
            after: Some(id.clone()),
            algorithm: None,
        };
        rsx! {
            button {
                key: "{kind}",
                style: BUTTON,
                onclick: move |_| {
                    run(&inserts, &track, &edit);
                    tick += 1;
                },
                "{kind}"
            }
        }
    });
    rsx! {
        div {
            style: "display:flex; flex-direction:row; align-items:center; gap:6px; flex-wrap:wrap;",
            span { style: "font-size:12px; color:#9ca3af; margin-right:4px;", "Add after:" }
            {kinds}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(names: &[&str]) -> Vec<BlockId> {
        names.iter().map(|n| BlockId((*n).to_owned())).collect()
    }

    /// A move up or down names the block it then follows.
    #[test]
    fn a_move_names_what_it_follows() {
        let phase = ids(&["a", "b", "c"]);
        let b = BlockId("b".into());
        assert_eq!(moved_after(&phase, &b, -1), Some(None));
        assert_eq!(moved_after(&phase, &b, 1), Some(Some(BlockId("c".into()))));
        let a = BlockId("a".into());
        assert_eq!(moved_after(&phase, &a, -1), None);
        let c = BlockId("c".into());
        assert_eq!(moved_after(&phase, &c, 1), None);
        assert_eq!(moved_after(&phase, &c, -1), Some(Some(BlockId("a".into()))));
    }
}
