//! The live session as a Loro CRDT document, the primitive the rest of
//! Studio agrees on: a song's tracks, items, markers, its sections (as
//! regions), tempo map and chart, in one document that every device and
//! every view edits and merges field by field.
//!
//! Copied, unchanged except for paths, from the studio repo's
//! `features/session/src/sync` (commit 0fae0b4d7): its engine- and
//! transport-free half (the schema, the model, the diff, presence, the
//! shared transport, crop, hold and align). The bridge to a DAW
//! engine, the relay (architect's transport), the network and the
//! shared clock stay there. One change:
//! [`Position`] stands in for `daw_transport_sync::Position`, with the
//! same fields, so this crate needs nothing from the DAW repo, and
//! `web_time::Instant` for `architect::platform::Instant`.
//!
//! Each replica is the source of truth for its own engine: local edits
//! are read back as a [`SessionModel`] and reconciled into the doc
//! ([`SessionDoc::write`]); remote edits arrive as Loro updates, and
//! [`diff`] between the model last shown and the doc's new one says what
//! must change. Here, on Drive, a song's document is a Drive document
//! (`studio:song:<id>`), wrapped with [`SessionDoc::from_loro`].

pub mod align;
pub mod crop;
pub mod diff;
pub mod doc;
pub mod hold;
pub mod model;
pub mod presence;
pub mod slug;
pub mod transport;

pub use diff::{Change, ItemField, TrackField, diff};
pub use doc::{ORIGIN_LOCAL, SCHEMA, SessionDoc};
pub use hold::{Held, Hold};
pub use loro;
pub use model::{
    AutomationItemState, CcState, ChannelPressureState, CompAreaState, CompingState,
    EnvelopePointState, EnvelopeState, FxState, GainPoint, HitState, ItemState, ItemTiming,
    LaneState, MarkerState, MasterState, MidiEvents, NoteExpressionState, NoteState,
    PitchBendState, PolyPressureState, ProgramChangeState, ProjectSettings, RegionState,
    RulerLaneState, SendState, SessionModel, Sources, StretchPoint, SysexState, TakeMarkerState,
    TakeState, TempoPoint, TrackState,
};

/// A transport's position at a moment (as `daw_transport_sync::Position`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// When, in the shared clock (µs).
    pub host_micros: f64,
    pub playhead_seconds: f64,
    pub playrate: f64,
    pub is_playing: bool,
}
