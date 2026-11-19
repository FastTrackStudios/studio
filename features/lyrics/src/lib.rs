//! Studio's Lyrics, an Architect extension: the timed lyrics of the song
//! playing as Session's stage display, following the one playhead
//! (`extension.toml`; the extension in [`ext`], its view in [`view`]).
//!
//! The song and the clock are setlists' (`setlists::now`, read as
//! `studio.setlists`' `now`). Its lines are the song's one lyrics source
//! (`setlists::workspace::lyrics_source`: its `.lrc`, else its chart's
//! timed lyric lane), drawn against the chart's sections as played once
//! the chart is engraved, never waiting on it.

pub mod ext;
pub mod view;
