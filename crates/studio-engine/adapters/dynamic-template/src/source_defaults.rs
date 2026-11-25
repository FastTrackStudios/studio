//! The default balance a guitar configuration's sources start at.
//!
//! `flow.guitars.mixing.source-defaults` states the rule as a property of
//! a *configuration*, not of a gesture: given the sources under one
//! channel, each one has a pan and a mute it starts at. This module is
//! that rule as pure data — no DAW, no tracks — so every place that
//! creates sources applies the same code rather than its own copy. Today
//! that is the session grow actions (`session::guitar_grow`); the golden
//! builder ([`crate::golden_session`]) is the other one, and calls in
//! here when its guitar parts gain the sources `flow.guitars.golden`
//! describes. It lives in this crate, beside the builder, for that
//! reason.
//!
//! # Applied once, at creation
//!
//! The decision on #37 is "source defaults apply once, at creation;
//! nothing re-applies, so a moved source keeps its place by
//! construction; no flag". The spec's own wording is the operational
//! form of that: "a source a user has moved keeps its place; the default
//! applies to **what has never been set**".
//!
//! [`Balance::is_untouched`] is what "never been set" means without a
//! flag: a track still at pan centre and unmuted has never been
//! balanced, and a track that has been balanced is not one of those. A
//! caller applies [`channel_defaults`] only to the tracks it just created
//! plus the siblings still reading untouched — which is why adding a
//! second amp can move the first amp off centre, while a source the user
//! has panned anywhere at all is left exactly where they put it.

/// Hard left, the pan a 57 starts at.
pub const HARD_LEFT: f64 = -1.0;
/// Hard right, the pan a 121 starts at.
pub const HARD_RIGHT: f64 = 1.0;
/// How far an amp's own track leans to its side when a channel has more
/// than one amp — 60 %, so both mics of both amps are heard.
pub const AMP_SPREAD: f64 = 0.6;

/// Where one track sits: its pan and whether it starts muted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Balance {
    /// -1.0 hard left, 0.0 centre, 1.0 hard right.
    pub pan: f64,
    /// Whether the track starts muted.
    pub muted: bool,
}

impl Balance {
    /// Centred and audible — what a track reads as before anything has
    /// balanced it.
    pub const CENTRED: Self = Self {
        pan: 0.0,
        muted: false,
    };

    /// Read a track's pan and mute as a balance.
    #[must_use]
    pub const fn of(pan: f64, muted: bool) -> Self {
        Self { pan, muted }
    }

    /// Has this balance never been set?
    ///
    /// The no-flag test for "the default applies to what has never been
    /// set": centred and unmuted is the state a track is created in, so
    /// a track reading it has not been balanced by anyone.
    ///
    /// The cost of having no flag is here and is deliberate: an engineer
    /// who un-mutes a DI and leaves it centred is indistinguishable from
    /// one who never touched it, and the next source added to that
    /// channel will mute it again. Moving it anywhere at all — which is
    /// what "a source a user has moved keeps its place" says — makes the
    /// choice legible and it is then left alone.
    #[must_use]
    pub fn is_untouched(self) -> bool {
        !self.muted && self.pan.abs() < f64::EPSILON
    }
}

/// One source under a channel, with the mics it carries.
///
/// A DI or a pedalboard is a leaf (`mics` empty); an amp is a folder over
/// its 57 and its 121.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The source track's name, from the template's `MultiMic`
    /// vocabulary — `DI`, `Pedalboard`, `Amp 1`, `Neck`, ...
    pub name: String,
    /// The mic tracks under it, if it is a folder over mics.
    pub mics: Vec<String>,
}

impl Source {
    /// A source carrying no mics of its own.
    #[must_use]
    pub fn leaf(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            mics: Vec::new(),
        }
    }

    /// A source that is a folder over `mics`.
    #[must_use]
    pub fn with_mics<S: Into<String>>(name: impl Into<String>, mics: Vec<S>) -> Self {
        Self {
            name: name.into(),
            mics: mics.into_iter().map(Into::into).collect(),
        }
    }
}

/// A track under a channel, addressed by the names on the way down to it,
/// and the balance it starts at.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceBalance {
    /// `["Amp 1"]` for the amp's own track, `["Amp 1", "57"]` for its 57.
    pub path: Vec<String>,
    /// Where that track starts.
    pub balance: Balance,
}

/// What each source role means for the balance, read off a source name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Di,
    Pedalboard,
    Amp,
    Mic57,
    Mic121,
    Other,
}

fn role(name: &str) -> Role {
    let lower = name.trim().to_lowercase();
    if lower == "di" || lower.starts_with("di ") || lower.ends_with(" di") {
        Role::Di
    } else if lower.contains("pedal") || lower == "board" {
        Role::Pedalboard
    } else if lower.contains("amp") {
        Role::Amp
    } else if lower.contains("121") || lower.contains("royer") || lower.contains("ribbon") {
        Role::Mic121
    } else if lower.contains("57") {
        Role::Mic57
    } else {
        Role::Other
    }
}

/// The mics a newly created source arrives with.
///
/// An amp is never a bare track in this vocabulary — it is the folder
/// over the pair of mics on it, which is what makes the hard-L/R rule
/// something a configuration has rather than something a user sets.
#[must_use]
pub fn default_mics(source_name: &str) -> Vec<String> {
    match role(source_name) {
        Role::Amp => vec!["SM57".to_string(), "Royer".to_string()],
        _ => Vec::new(),
    }
}

/// The default balance of every track under one channel.
///
/// `sources` is the channel's whole source list in mixer order, including
/// the ones that already exist — the rule reads the *configuration*, so a
/// DI is only muted once something else is beside it and an amp only
/// leans once a second amp arrives.
#[must_use]
pub fn channel_defaults(sources: &[Source]) -> Vec<SourceBalance> {
    let amp_count = sources
        .iter()
        .filter(|s| role(&s.name) == Role::Amp)
        .count();
    let multiple_sources = sources.len() > 1;
    let has_amp = amp_count > 0;

    let mut amp_ordinal = 0usize;
    let mut out = Vec::new();
    for source in sources {
        let source_role = role(&source.name);
        let mut balance = Balance::CENTRED;
        match source_role {
            // The reamp and the safety, not the sound.
            Role::Di if multiple_sources => balance.muted = true,
            // The amp — the "Main" — takes priority.
            Role::Pedalboard if has_amp => balance.muted = true,
            // Two amps: each leans to its own side so both are heard.
            Role::Amp if amp_count > 1 => {
                balance.pan = if amp_ordinal.is_multiple_of(2) {
                    -AMP_SPREAD
                } else {
                    AMP_SPREAD
                };
            }
            _ => {}
        }
        if source_role == Role::Amp {
            amp_ordinal = amp_ordinal.saturating_add(1);
        }
        out.push(SourceBalance {
            path: vec![source.name.clone()],
            balance,
        });

        let several_mics = source.mics.len() > 1;
        for mic in &source.mics {
            let mut mic_balance = Balance::CENTRED;
            match role(mic) {
                Role::Mic57 => mic_balance.pan = HARD_LEFT,
                Role::Mic121 => mic_balance.pan = HARD_RIGHT,
                Role::Di if several_mics => mic_balance.muted = true,
                _ => {}
            }
            out.push(SourceBalance {
                path: vec![source.name.clone(), mic.clone()],
                balance: mic_balance,
            });
        }
    }
    out
}
