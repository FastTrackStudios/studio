//! The track panel's automation button: a list of what the track can
//! automate.
//!
//! Its volume, pan, width and mute, and each plugin's parameters a plugin
//! at a time — each ticked where it is shown in a lane, a click showing or
//! hiding it.

use daw_proto::automation::EnvelopeRef;
use daw_proto::primitives::AutomationMode;

use crate::popup::Choice;

/// What a choice in the automation menu does.
#[derive(Clone, Debug)]
pub enum Pick {
    /// Show (or hide) the envelope named `envelope` (as
    /// [`crate::automation_actions::find`] reads names).
    Toggle {
        track: String,
        envelope: String,
        show: bool,
    },
    /// Open plugin `fx` (its place in the chain, from 1)'s parameters.
    Fx { track: String, fx: usize },
    /// Back to the first list.
    Back { track: String },
    /// Expand (or fold) the vocal leveler: its four stages a lane each, or
    /// only their sum on the items.
    Leveler { track: String, show: bool },
    /// Show (or hide) the track's items' volume curves on them.
    TakeLines { track: String, show: bool },
    /// An envelope's automation mode, or (`envelope` `None`) the track's —
    /// what an envelope not made yet writes by.
    Mode {
        track: String,
        envelope: Option<EnvelopeRef>,
        mode: AutomationMode,
    },
}

/// The modes, as a mixer names them.
const MODES: [(&str, AutomationMode); 6] = [
    ("Trim/Read", AutomationMode::TrimRead),
    ("Read", AutomationMode::Read),
    ("Touch", AutomationMode::Touch),
    ("Latch", AutomationMode::Latch),
    ("Write", AutomationMode::Write),
    ("Off", AutomationMode::Off),
];

/// The modes for an envelope (or the track's, `None`), the one it is in
/// ticked.
#[must_use]
pub fn modes(
    track: &str,
    envelope: Option<&EnvelopeRef>,
    now: AutomationMode,
) -> Vec<Choice<Pick>> {
    MODES
        .iter()
        .map(|(label, mode)| {
            Choice::new(
                *label,
                *mode == now,
                Pick::Mode {
                    track: track.to_owned(),
                    envelope: envelope.cloned(),
                    mode: *mode,
                },
            )
        })
        .collect()
}

/// What the engine says `track`'s menu lists (`AUTOMATION_MENU`), with
/// plugin `fx`'s parameters when asked; `None` when it would not say.
fn facts(track: &str, fx: Option<usize>) -> Option<crate::automation_actions::MenuFacts> {
    let args = serde_json::json!({ "track": track, "fx": fx });
    let said = crate::actions::run("AUTOMATION_MENU", args.to_string().as_bytes())
        .inspect_err(|e| tracing::warn!(error = %e, "automation menu: the engine did not say"))
        .ok()?;
    facet_json::from_slice(&said).ok()
}

/// The first list: the track's own envelopes, then its plugins.
#[must_use]
pub fn top(track: &str) -> Vec<Choice<Pick>> {
    use daw_proto::automation::EnvelopeType as T;
    let Some(facts) = facts(track, None) else {
        return Vec::new();
    };
    let on = &facts.shown;
    let mut out = vec![Choice::heading("Track")];
    for (label, ty) in [
        ("Volume", T::Volume),
        ("Pan", T::Pan),
        ("Width", T::Width),
        ("Mute", T::Mute),
        ("Volume pre-FX", T::VolumePrefx),
    ] {
        let is_on = on.contains(&EnvelopeRef::Type(ty));
        out.push(Choice::new(
            label,
            is_on,
            Pick::Toggle {
                track: track.to_owned(),
                envelope: label.to_owned(),
                show: !is_on,
            },
        ));
    }
    out.push(Choice::new(
        "Item volume curves",
        facts.takes,
        Pick::TakeLines {
            track: track.to_owned(),
            show: !facts.takes,
        },
    ));
    if let Some(is_on) = facts.leveler {
        out.push(Choice::new(
            crate::automation::LEVELER,
            is_on,
            Pick::Leveler {
                track: track.to_owned(),
                show: !is_on,
            },
        ));
    }
    // The track's mode: how a fader move writes where no envelope is yet.
    out.push(Choice::heading("Track mode"));
    out.extend(modes(
        track,
        None,
        facts.mode.unwrap_or(AutomationMode::TrimRead),
    ));
    if !facts.fxs.is_empty() {
        out.push(Choice::heading("Effects"));
    }
    for (i, (guid, name)) in facts.fxs.iter().enumerate() {
        let automated = on
            .iter()
            .any(|t| matches!(t, EnvelopeRef::FxParam { fx_guid, .. } if fx_guid == guid));
        let mut choice = Choice::more(
            name.clone(),
            Pick::Fx {
                track: track.to_owned(),
                fx: i.saturating_add(1),
            },
        );
        choice.ticked = automated;
        out.push(choice);
    }
    out
}

/// One plugin's parameters.
#[must_use]
pub fn params(track: &str, fx: usize) -> Vec<Choice<Pick>> {
    let Some(facts) = facts(track, Some(fx)) else {
        return Vec::new();
    };
    let Some((guid, name)) = facts.fxs.get(fx.saturating_sub(1)) else {
        return Vec::new();
    };
    let mut out = vec![
        Choice::new(
            "Back",
            false,
            Pick::Back {
                track: track.to_owned(),
            },
        ),
        Choice::heading(name.clone()),
    ];
    for (index, param) in &facts.params {
        let is_on = facts.shown.contains(&EnvelopeRef::FxParam {
            fx_guid: guid.clone(),
            param_index: *index,
        });
        out.push(Choice::new(
            param.clone(),
            is_on,
            Pick::Toggle {
                track: track.to_owned(),
                envelope: format!("#{fx}/{param}"),
                show: !is_on,
            },
        ));
    }
    out
}

/// Run `id` on the engine with `args`, behind the window; the engine's
/// handler asks the windows to read the song back.
fn ask(id: &'static str, args: serde_json::Value) {
    crate::actions::off_the_window(id, args.to_string());
}

/// Do what `pick` says: the next list to show, or `None` when the menu is
/// done.
#[must_use]
pub fn act(pick: &Pick) -> Option<Vec<Choice<Pick>>> {
    match pick {
        Pick::Fx { track, fx } => Some(params(track, *fx)),
        Pick::Back { track } => Some(top(track)),
        Pick::Mode {
            track,
            envelope,
            mode,
        } => {
            let Ok(target) = facet_json::to_string(envelope) else {
                return None;
            };
            let Ok(mode) = facet_json::to_string(mode) else {
                return None;
            };
            crate::actions::off_the_window(
                "AUTOMATION_SET_MODE",
                format!(
                    r#"{{"track":{},"target":{target},"mode":{mode}}}"#,
                    serde_json::Value::from(track.as_str())
                ),
            );
            None
        }
        Pick::TakeLines { track, show } => {
            ask(
                "AUTOMATION_SHOW",
                serde_json::json!({ "track": track, "envelope": "Item volume", "shown": show }),
            );
            None
        }
        Pick::Leveler { track, show } => {
            ask(
                "AUTOMATION_SHOW",
                serde_json::json!({ "track": track, "envelope": crate::automation::LEVELER, "shown": show }),
            );
            None
        }
        Pick::Toggle {
            track,
            envelope,
            show,
        } => {
            ask(
                "AUTOMATION_SHOW",
                serde_json::json!({ "track": track, "envelope": envelope, "shown": show }),
            );
            None
        }
    }
}
