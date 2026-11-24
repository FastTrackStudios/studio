//! The arrangement's link into the editor: the item a plain click picked
//! for editing, handed to whatever editor the app installed. The
//! arrangement only says what was picked ([`picked`]); the app wires that
//! to its editor ([`install`], the MIDI editor's `editor_view::install`),
//! so the arrangement never names the editor. With nothing installed a
//! pick goes nowhere.

use std::sync::OnceLock;

/// What the app does with a picked item: its GUID.
pub type Pick = fn(&str);

static TARGET: OnceLock<Pick> = OnceLock::new();

/// Send every item the arrangement picks to `pick`: the app's editor.
/// Once per process; a second install keeps the first.
pub fn install(pick: Pick) {
    let _ = TARGET.set(pick);
}

/// The arrangement picked `item` for editing (a plain click selects it
/// alone): the installed editor follows it.
pub fn picked(item: &str) {
    if let Some(pick) = TARGET.get() {
        pick(item);
    }
}
