//! Organize mode's instrument rail: Drums, Bass, Guitar down the
//! arrangement's left — each press adds that instrument's tracks
//! (`crate::instrument_tracks`).

use dioxus::prelude::*;

use crate::instrument_tracks::Instrument;
use crate::shell::{DIM, RULE, TEXT};

/// Organize mode's instruments, down the left of the arrangement: each
/// button adds its instrument to the song (`TRACK_ADD_INSTRUMENT`).
#[component]
pub fn InstrumentRail() -> Element {
    rsx! {
        div {
            style: "flex:none; width:64px; display:flex; flex-direction:column; gap:6px; \
                    padding:8px 6px; box-sizing:border-box; background:#141518; \
                    border-right:1px solid {RULE};",
            div {
                style: "font-size:9px; font-weight:700; letter-spacing:0.06em; color:{DIM}; \
                        text-align:center; padding-bottom:2px;",
                "ADD"
            }
            for instrument in Instrument::ALL {
                button {
                    title: instrument.title(),
                    style: "flex:none; height:30px; padding:0 4px; border-radius:5px; \
                            border:1px solid {RULE}; background:#1c1e22; color:{TEXT}; \
                            font-size:11px; font-weight:600; cursor:pointer; white-space:nowrap;",
                    onclick: move |_| press(instrument),
                    "{instrument.name()}"
                }
            }
        }
    }
}

/// A button of the column: its instrument added, through the Action List.
pub(crate) fn press(instrument: Instrument) {
    let args = format!(r#"{{"instrument":"{}"}}"#, instrument.name());
    if let Err(e) = crate::actions::run(crate::instrument_tracks::ADD.id, args.as_bytes()) {
        tracing::warn!(
            track.instrument = instrument.name(),
            error = %e,
            "organize: the instrument was not added"
        );
    }
}
