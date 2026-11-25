//! The live session as a Loro CRDT document, the primitive the rest of
//! Studio agrees on: a song's tracks, items, markers, its sections (as
//! regions), tempo map and chart, in one document that every device and
//! every view edits and merges field by field.
//!
//! The engine-free half of the old Session app's `sync`: the schema, the
//! model, the diff, presence, the shared transport, crop, hold, align,
//! drift, the relay between two documents and (feature `net`) the network
//! lanes. The bridge to a DAW engine and the shared clock are
//! `engine-sync`'s (the engine group). One change from the original:
//! `web_time::Instant` for `architect::platform::Instant`.
//!
//! Each replica is the source of truth for its own engine: local edits
//! are read back as a [`SessionModel`] and reconciled into the doc
//! ([`SessionDoc::write`]); remote edits arrive as Loro updates, and
//! [`diff`] between the model last shown and the doc's new one says what
//! must change. Here, on Drive, a song's document is a Drive document
//! (`studio:song:<id>`), wrapped with [`SessionDoc::from_loro`].

pub mod align;
#[cfg(feature = "net")]
pub mod clock;
pub mod crop;
pub mod diff;
pub mod doc;
pub mod drift;
pub mod hold;
pub mod model;
#[cfg(feature = "net")]
pub mod net;
/// Who else is here: the session primitive's.
pub use fts_session::presence;
pub mod relay;
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

/// A transport's position at a moment: the DAW transport's own type.
pub use daw_transport_sync::Position;
