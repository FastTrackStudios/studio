//! A song as the player and the pages carry it: its title and the facts
//! known of it (a setlist file's song knows only its title until its
//! session is read).

use std::collections::HashMap;

/// A song in a list being played or shown.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Song {
    pub title: &'static str,
    pub artist: Option<&'static str>,
    pub key: Option<&'static str>,
    pub tempo: Option<u32>,
    /// Its length in seconds, when known.
    pub len: Option<u32>,
    /// What it is ready with ([`crate::look::READY`]'s letters).
    pub ready: &'static str,
}

/// Text kept for the life of the app, once per value: a [`Song`]'s
/// fields are `'static` so lists of them copy cheaply.
fn keep(text: &str) -> &'static str {
    thread_local! {
        static KEPT: std::cell::RefCell<HashMap<String, &'static str>> =
            std::cell::RefCell::new(HashMap::new());
    }
    KEPT.with(|t| {
        *t.borrow_mut()
            .entry(text.to_string())
            .or_insert_with(|| Box::leak(text.to_string().into_boxed_str()))
    })
}

/// A song read at run time (a setlist file's) by its title alone.
pub fn song_named(title: &str) -> Song {
    Song {
        title: keep(title),
        artist: None,
        key: None,
        tempo: None,
        len: None,
        ready: "",
    }
}
