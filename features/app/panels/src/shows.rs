//! Studio's modes, as what they ask the views to show
//! (`daw_view_kit::shows`): a mode only shows or hides parts of the views.

use daw_view_kit::shows::Shows;
use dioxus::prelude::*;
use session::modes::Mode;

/// What `mode` asks the views to show: the arm buttons in Record, every
/// item in its track's colour in Mix, every cut (and the drum detection,
/// the comp strip) in Edit and in Record, where takes are comped as they
/// come in.
#[must_use]
pub fn of(mode: Mode) -> Shows {
    Shows {
        arm: mode == Mode::Record,
        item_colors: mode == Mode::Mix,
        cuts: matches!(mode, Mode::Edit | Mode::Record),
        mode: Some(mode.slug()),
        mixer_open: mode != Mode::Organize,
        mix: mode == Mode::Mix,
        record: mode == Mode::Record,
    }
}

/// Provide what the window's mode asks the views to show, following it:
/// once, where the window's mode is made.
pub fn provide(mode: Signal<Mode>) {
    use_context_provider(|| Memo::new(move || of(mode())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_arms_and_comps_mix_colours_live_shows_neither() {
        assert!(of(Mode::Record).arm && of(Mode::Record).cuts);
        assert!(of(Mode::Mix).item_colors && !of(Mode::Mix).cuts);
        assert_eq!(
            of(Mode::Live),
            Shows {
                arm: false,
                item_colors: false,
                cuts: false,
                mode: Some(Mode::Live.slug()),
                mixer_open: true,
                mix: false,
                record: false,
            }
        );
    }
}
