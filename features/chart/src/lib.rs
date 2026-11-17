//! Studio's Chart, an Architect extension: the engraved chart of the song
//! playing, following the one playhead (`extension.toml`; the extension in
//! [`ext`], its view in [`view`]).
//!
//! The song and the clock are setlists' (`setlists::now`, read as
//! `studio.setlists`' `now`): the song playing is the item on the
//! playhead, its time the item's. Its chart is the song's `.kf`, or the
//! arrangement's (`Arrangements/<name>/`) when the setlist's row plays
//! one, engraved by Keyflow off the view's thread, timed on the chart's
//! own clock (`setlists::follow`), the bar and the beat at the playhead
//! marked (Keyflow's playback cursor).

pub mod ext;
pub mod view;
