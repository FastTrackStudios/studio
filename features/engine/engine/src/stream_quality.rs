//! How a streamed song's recordings are heard, per device: always from its
//! Ogg proxies first; then, when this device asks for full resolution, from
//! the original recordings once they have come down behind everything else.
//!
//! - [`Quality::Proxies`] (the default): the proxies only. An original is
//!   never fetched.
//! - [`Quality::Full`]: once every song of the set has all its proxies here
//!   (and none is still heard by its reference), the originals are fetched
//!   one at a time, the lowest priority there is, and each take moves from
//!   its proxy to its original when its file is complete and its song is
//!   stopped ([`crate::stream_set`]). Natively only: a browser has no disk
//!   to keep a set's WAVs on, and a share link serves an original whole.
//!
//! Kept with `utils::prefs` (`stream-quality`): a file under the config
//! dir natively, `localStorage` in a browser.

/// Where the choice is kept.
const KEY: &str = "stream-quality";

/// What a streamed song is heard from, once it can be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quality {
    /// The proxies only: never an original.
    #[default]
    Proxies,
    /// The originals, where they can be had, after every proxy is here.
    Full,
}

impl Quality {
    /// Both, in the order a setting lists them.
    pub const ALL: [Self; 2] = [Self::Proxies, Self::Full];

    /// The word it is kept and asked for by.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Proxies => "proxies",
            Self::Full => "full",
        }
    }

    /// What a setting calls it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Proxies => "Proxies only",
            Self::Full => "Full resolution when available",
        }
    }

    /// What it means, in a sentence.
    #[must_use]
    pub const fn blurb(self) -> &'static str {
        match self {
            Self::Proxies => {
                "Streamed songs play their compressed proxies — a few MB a track. The original recordings are never downloaded."
            }
            Self::Full => {
                "Streamed songs start on their proxies; once every song's proxies are here, the original recordings come down behind them, one at a time, and each track moves to its original the next time its song is stopped."
            }
        }
    }

    /// The quality `text` names (`proxies`, `full`), any case.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        Self::ALL
            .into_iter()
            .find(|q| q.key().eq_ignore_ascii_case(text))
    }
}

/// This device's choice; [`Quality::Proxies`] when it never made one.
#[must_use]
pub fn current() -> Quality {
    utils::prefs::get_raw(KEY)
        .and_then(|raw| Quality::parse(&raw))
        .unwrap_or_default()
}

/// Keep `quality` as this device's choice. What streams now takes it up
/// on its next look ([`crate::stream_set`] checks before each original).
pub fn set(quality: Quality) {
    utils::prefs::set_raw(KEY, quality.key());
}

#[cfg(test)]
mod tests {
    use super::Quality;

    #[test]
    fn a_quality_parses_the_word_it_is_kept_by() {
        for q in Quality::ALL {
            assert_eq!(Quality::parse(q.key()), Some(q));
        }
        assert_eq!(Quality::parse(" FULL "), Some(Quality::Full));
        assert_eq!(Quality::parse("originals"), None);
        assert_eq!(Quality::default(), Quality::Proxies);
    }
}
