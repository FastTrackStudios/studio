//! What Organize mode's instrument column adds, and the action it presses.
//! Adding the tracks is the engine's (`studio_engine::instrument_tracks`).

#[cfg(feature = "native")]
use architect::action::ActionMeta;

#[cfg(feature = "native")]
pub static ADD: ActionMeta = crate::actions::meta(
    "TRACK_ADD_INSTRUMENT",
    "track_add_instrument",
    "Add an instrument",
    "Add an `instrument`'s tracks at the top level, routed to its bus: `Drums` the golden template's whole kit (each piece's Sum, close mics, Trig, Sample and Fund, the toms, cymbals, rooms and Process) with a `Drums/MIDI` track first in it for the drawn part, folded to one row; `Bass` one `Bass` track; `Guitar` one `Guitar` track. Refused when the song has a top-level track of that name already. One undo step. Says what it made and where it goes.",
    "Tracks",
    "",
);

/// What Organize mode's instrument column adds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Instrument {
    Drums,
    Bass,
    Guitar,
}

impl Instrument {
    /// The column's buttons, top to bottom.
    pub const ALL: [Self; 3] = [Self::Drums, Self::Bass, Self::Guitar];

    /// Its name, and the top-level track's.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Drums => "Drums",
            Self::Bass => "Bass",
            Self::Guitar => "Guitar",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Drums => "Add the drum kit — the golden template's, folded to one row",
            Self::Bass => "Add a Bass track, to the bass bus",
            Self::Guitar => "Add a Guitar track, to the electric bus",
        }
    }

    /// The instrument `text` names, loosely: `drums`, `kit`, `Guitars`.
    pub fn named(text: &str) -> Option<Self> {
        let text = text.trim();
        let plural = |name: &str| {
            text.strip_suffix(['s', 'S'])
                .is_some_and(|one| one.eq_ignore_ascii_case(name))
        };
        Self::ALL
            .into_iter()
            .find(|i| text.eq_ignore_ascii_case(i.name()) || plural(i.name()))
            .or_else(|| {
                ["kit", "drum", "drum kit"]
                    .iter()
                    .any(|k| text.eq_ignore_ascii_case(k))
                    .then_some(Self::Drums)
            })
    }
}
