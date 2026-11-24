//! What an arrangement and a mixer beside it share, and the mixer's
//! choices the rest of the window reads: the docked pair's [`Links`]
//! (rows, edits each way, keys), whether the mixer is open
//! ([`MixerMemory`]), and whether it shows its racks ([`racks_shown`]).
//!
//! Types only, so the arrangement reaches none of the mixer's code: the
//! mixer (`mixer_panel`) provides the links, the arrangement takes them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use dioxus::prelude::*;

use crate::engine::Edit;

pub type Rows = Vec<(daw_proto::Track, u32)>;
pub type Queue = Rc<RefCell<Vec<Edit>>>;

/// What the arrangement and the mixer share. Provided by the mixer panel (`mixer_panel::DawPanels`);
/// the arrangement takes it if it is there.
#[derive(Clone)]
pub struct Links {
    /// Whether the mixer is showing.
    pub open: Signal<bool>,
    /// Docked: the arrangement and the mixer are one pair in a view that
    /// shares its window with more (the Overview), so the arrangement
    /// opens with the compact track panel. The DAW view's is full.
    pub docked: bool,
    /// Asked for by a widget's key (`x`), carried out by the arrangement
    /// panel once a frame, since a widget cannot set a signal.
    pub toggle: Rc<Cell<bool>>,
    /// The window's one way to open the mixer, when the shell has one:
    /// what `x` presses (`crate::shell::MixerToggle`).
    pub switch: Option<crate::shell::MixerToggle>,
    /// The arrangement's rows, with a generation that goes up on every
    /// change, so the mixer knows to re-record.
    pub rows: Rc<RefCell<(u64, Rows)>>,
    /// How the song's rows are planned, for a mixer with no arrangement to
    /// plan them: a folder opened or shut plans them again here.
    pub planner: Option<crate::song_model::Planner>,
    /// Edits the mixer made, for the engine and the arrangement.
    pub from_mixer: Queue,
    /// Edits the arrangement made, for the mixer to apply too.
    pub to_mixer: Queue,
    /// Edits the mixer made, for the arrangement to apply too.
    pub to_arrange: Queue,
    /// Keys pressed while the mixer has the focus, for the arrangement:
    /// the keyboard is one keymap whichever panel was clicked last, and
    /// the arrangement is where it is read.
    pub keys: Rc<RefCell<Vec<Key>>>,
    /// The arrangement's node. A click in the mixer takes the focus off
    /// it without giving it to anything that reads keys, so the mixer
    /// hands it straight back: the keyboard is the arrangement's.
    pub arrange_node: Rc<RefCell<Option<Rc<MountedData>>>>,
    /// No arrangement beside it — the phone's Mixer view, the record view
    /// — so nothing else carries the mixer's edits to the engine: it
    /// sends them itself.
    pub alone: bool,
    /// Strips whose record arm arms another track, by the strip's guid.
    /// The record view's groups are folders, and a folder's arm is the
    /// track under it that records; the strip shows that track's arm.
    pub arm_of: Rc<std::collections::HashMap<String, daw_proto::Track>>,
    /// A few strips given more room than they need (the record view's
    /// five): drawn as big as fills it, rather than leaving it empty.
    pub fill: bool,
    /// The inspector: of the rows, only the selected track and the folder
    /// it sits in — its own strip and the one it goes out through, as
    /// Logic's inspector shows them beside the tracks.
    pub inspector: bool,
    /// How many strips the inspector shows (one, or two with the folder
    /// the track goes out through): its column is as wide as they are.
    pub strips: Rc<Cell<usize>>,
    /// The mixer alone in Mix mode: laid out by the scene for mixing,
    /// racks on every strip wide enough, framed by the phase rails — what
    /// the scene renders (`tests/golden_scenes.rs`). Set every render, as
    /// the mode changes.
    pub mix: Rc<Cell<bool>>,
    /// Record mode, docked or alone: full strips, laid out for recording.
    /// Set every render, as the mode changes.
    pub recording: Rc<Cell<bool>>,
    /// Mix mode, docked or alone: full strips, laid out for mixing (the
    /// rails and the scene are the mixer alone's — `mix`).
    pub mixing: Rc<Cell<bool>>,
}

/// A key the mixer passed on.
#[derive(Clone, Debug)]
pub enum Key {
    Down(blitz_traits::events::BlitzKeyEvent),
    Up(blitz_traits::events::BlitzKeyEvent),
}

impl Links {
    #[must_use]
    pub fn new(rows: Rows) -> Self {
        Self {
            open: Signal::new(false),
            docked: false,
            toggle: Rc::new(Cell::new(false)),
            switch: None,
            rows: Rc::new(RefCell::new((0, rows))),
            planner: None,
            from_mixer: Queue::default(),
            to_mixer: Queue::default(),
            to_arrange: Queue::default(),
            keys: Rc::default(),
            arrange_node: Rc::default(),
            alone: false,
            arm_of: Rc::default(),
            fill: false,
            inspector: false,
            strips: Rc::new(Cell::new(1)),
            mix: Rc::new(Cell::new(false)),
            recording: Rc::new(Cell::new(false)),
            mixing: Rc::new(Cell::new(false)),
        }
    }

    /// A mixer on its own: `rows` its strips, open, sending its edits to
    /// the engine itself, with `arm_of`'s strips arming other tracks.
    #[must_use]
    pub fn alone(rows: Rows, arm_of: std::collections::HashMap<String, daw_proto::Track>) -> Self {
        let mut links = Self::new(rows);
        links.open = Signal::new(true);
        links.alone = true;
        links.arm_of = Rc::new(arm_of);
        links
    }
}

/// Whether the mixer is open, remembered per mode and per view (docked in
/// the Overview or summoned in the DAW view) — a context the shell
/// provides above the songs, so picking another song keeps it.
///
/// Unremembered, the Overview's docked mixer is open except in Organize,
/// where the room goes to the arrangement; the DAW view's is closed.
#[derive(Clone, Copy, PartialEq)]
pub struct MixerMemory(pub Signal<std::collections::HashMap<(bool, session::modes::Mode), bool>>);

impl MixerMemory {
    #[must_use]
    pub fn new() -> Self {
        Self(Signal::new(std::collections::HashMap::new()))
    }

    /// Open or closed, for panels `docked` or not, shown in `mode`.
    #[must_use]
    pub fn wanted(memory: Option<Self>, docked: bool, mode: Option<session::modes::Mode>) -> bool {
        let fallback = docked && mode != Some(session::modes::Mode::Organize);
        match (memory, mode) {
            (Some(memory), Some(mode)) => memory
                .0
                .peek()
                .get(&(docked, mode))
                .copied()
                .unwrap_or(fallback),
            _ => fallback,
        }
    }

    pub fn remember(mut self, docked: bool, mode: session::modes::Mode, open: bool) {
        if self.0.peek().get(&(docked, mode)) != Some(&open) {
            self.0.write().insert((docked, mode), open);
        }
    }
}

impl Default for MixerMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Where the racks' choice is kept (`utils::prefs`): `shown` or `hidden`.
const RACKS_PREF: &str = "mixer.fx-racks";

/// The racks' choice, read from the prefs once.
static RACKS: std::sync::LazyLock<std::sync::atomic::AtomicBool> = std::sync::LazyLock::new(|| {
    std::sync::atomic::AtomicBool::new(
        utils::prefs::get_raw(RACKS_PREF).as_deref() != Some("hidden"),
    )
});

/// Whether the mixer shows its FX racks over the faders (shown unless
/// hidden): hidden, no strip opens to a rack and the faders keep the
/// panel's whole height — Mix mode's racks too.
#[must_use]
pub fn racks_shown() -> bool {
    RACKS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Show or hide the mixer's FX racks, kept across launches. Every mixer
/// takes it up on its next frame.
pub fn set_racks_shown(on: bool) {
    RACKS.store(on, std::sync::atomic::Ordering::Relaxed);
    utils::prefs::set_raw(RACKS_PREF, if on { "shown" } else { "hidden" });
}
