//! `editor` — the portable engine and surfaces of the expression editor.
//!
//! One editor, two products:
//!
//! - **an MPE editor**, where per-note pitch bend, channel pressure,
//!   and CC74 are edited as properties *of a note* instead of in raw
//!   controller lanes;
//! - **a Melodyne competitor**, where an analyzed audio clip's notes
//!   carry a tracked f0 contour that edits by exactly the same
//!   gestures.
//!
//! They unify because a note in either domain is the same thing: an
//! integer pitch row plus a continuous curve measured in semitones
//! relative to it. See [`doc`] for why that framing is the whole trick,
//! and [`blob`] for the center/drift/vibrato decomposition that makes
//! Melodyne's two headline sliders work on hand-drawn MIDI too.
//!
//! The always-on core holds no UI, no DSP, and no DAW, so it compiles
//! for wasm, Blitz-native, and embedded builds alike and the interaction
//! model can be tested headlessly. Everything heavier is a cargo
//! feature.
//!
//! ## Map
//!
//! Core (always compiled; every module is also re-exported at the crate
//! root, so `editor::doc::Note` and `editor::model::doc::Note` are the
//! same path):
//!
//! - [`model`] — the document and what edits it: `doc`, `edit`, `rows`,
//!   `tracks`, `clipboard`, `cc`, `timing`.
//! - [`pitch`] — the pitch-curve vocabulary: `blob`, `draft`, `shape`,
//!   `modulation`, `tuning`, `reference`.
//! - [`drums`] — drum lanes: `drum`, `kit`, `fills`, `flam`.
//! - [`harmony`] — chords and the ChordGun degree model: `chord`, plus
//!   the chord-firing panel in `harmony::chords` (feature `chords`).
//! - [`interact`] — the interaction model: `camera`, `memagic`,
//!   `cursor`, `mouse`, `handles`, `tools`, `multitool`, `razor`,
//!   `menu`, `mode`, `actions`.
//!
//! Feature-gated surfaces: `ops` (document operations), `audio`
//! (analysis and alignment), `corpus`, `daw`, `demo`, `guitarpro`,
//! `host`, `paint`, `ui` (the Dioxus surface), `standalone`, `reaper`.

#[cfg(feature = "audio")]
pub mod audio;
#[cfg(feature = "corpus")]
pub mod corpus;
#[cfg(feature = "daw")]
pub mod daw;
#[cfg(feature = "demo")]
pub mod demo;
#[cfg(feature = "guitarpro")]
pub mod guitarpro;
#[cfg(feature = "host")]
pub mod host;
#[cfg(feature = "ops")]
pub mod ops;
#[cfg(feature = "paint")]
pub mod paint;
#[cfg(feature = "reaper")]
pub mod reaper;
#[cfg(feature = "standalone")]
pub mod standalone;
#[cfg(feature = "ui")]
pub mod ui;

pub mod drums;
pub mod harmony;
pub mod interact;
pub mod model;
pub mod pitch;

pub use drums::{drum, fills, flam, kit};
pub use harmony::chord;
pub use interact::{
    actions, camera, cursor, handles, memagic, menu, mode, mouse, multitool, razor, tools,
};
pub use model::{cc, clipboard, doc, edit, rows, timing, tracks};
pub use pitch::{blob, draft, modulation, reference, shape, tuning};

pub use camera::{Bounds, Camera, Content, VerticalCamera, Viewport};
pub use cc::{CcDisplay, CcLane, CcSet};
pub use chord::Chord;
pub use cursor::{Aim, Cursor};
pub use doc::{
    Curve, CurveShape, Dimension, ExpressionDoc, Marker, Note, NoteId, Point, Target, TimeBase,
};
pub use draft::PitchDraft;
pub use edit::{Edit, History};
pub use handles::{Handle, Scope};
pub use mode::{Mode, ModeFamily};
pub use mouse::{Action, MouseMap};
pub use multitool::{Bend, Steepness, Zone};
pub use reference::{MidiReference, RefNote, SnapSource};
pub use rows::{Articulation, DrumMap, NoteShape, RowSpace, StringTuning};
pub use shape::Shape;
pub use timeline::razor::{RazorArea, RazorSet};
pub use timing::{Separator, StretchLaw};
pub use tools::{Grid, Hit, Mods, Selection, Tool};
pub use tracks::{RefColor, Track, Workspace};
pub use tuning::{Temperament, Tuning};

mod editor;
pub use editor::{CUSHION, Editor, PAD, StripLane, UndoAsk, content_of};
