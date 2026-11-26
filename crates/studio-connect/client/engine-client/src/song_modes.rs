//! Each song's own mode (`engine_runtime::client::song_modes`, re-exported
//! here), and the hook that has the window's mode follow the song.

use dioxus::prelude::*;
use engine_domain::modes::Mode;

pub use engine_runtime::client::song_modes::*;

use crate::setlist::Setlist;

/// The window's mode follows the song: gone to, a song puts the window in
/// the mode it was last worked in — Live, when it has never been put in
/// another — and the mode changed while on it is what it keeps. Only a
/// change is remembered: a song merely visited in some mode is not that
/// mode's from then on. For a shell, over its setlist and its mode.
pub fn use_song_modes(setlist: Signal<Setlist>, mode: Signal<Mode>) {
    let mut mode = mode;
    // The song on screen, as it changes.
    let current = use_memo(move || setlist().current().map(|song| song.name.clone()));
    // The mode the song on screen was given when it was gone to: the
    // mode being that is the song's own doing, not a change to keep.
    let mut given = use_signal(|| None::<(String, Mode)>);
    use_effect(move || {
        if let Some(song) = current() {
            // `FTS_SESSION_MODE` names the mode the window opens in, so the
            // first song shown takes it over what it last kept.
            let opening = given.peek().is_none();
            let forced = std::env::var("FTS_SESSION_MODE").ok().and_then(|name| {
                Mode::ALL
                    .into_iter()
                    .find(|m| m.display_name().eq_ignore_ascii_case(&name))
            });
            let kept = forced
                .filter(|_| opening)
                .or_else(|| of(&song))
                .unwrap_or(Mode::Live);
            given.set(Some((song, kept)));
            if *mode.peek() != kept {
                mode.set(kept);
            }
        }
    });
    use_effect(move || {
        let now = mode();
        let Some(song) = current.peek().clone() else {
            return;
        };
        if given.peek().as_ref() != Some(&(song.clone(), now)) {
            remember(&song, now);
            given.set(Some((song, now)));
        }
    });
}
