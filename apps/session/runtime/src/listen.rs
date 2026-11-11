//! How a song is heard, and how deep a listener may go.
//!
//! [`Listen`] is the cap the embedding host passes in (a share link
//! offering only Reference Mode, or the whole multitrack); [`warm_order`]
//! is what a page brings in behind the song on screen under it.

/// How deep a listener may go into a song.
///
/// The cap the host that embeds the engine sets (a share link's layer).
/// The project is shown whole either way (every track and item, read-only where the stems are not
/// heard); the cap decides what media is fetched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Listen {
    /// Reference Mode only: the song's reference (its preview first), the
    /// click and guide live over it. No stem is fetched or decoded, and the
    /// multitracks are not offered. A song with no reference plays only
    /// what plays live (the guide).
    Reference,
    /// The multitracks may be heard: the stems come in behind the
    /// reference, and are offered — as before the cap existed.
    #[default]
    Full,
}

impl Listen {
    /// Whether a song's stems may be fetched and heard.
    #[must_use]
    pub const fn hears_stems(self) -> bool {
        matches!(self, Self::Full)
    }

    /// Whether what `heard` names may be fetched.
    #[must_use]
    pub const fn allows(self, heard: Heard) -> bool {
        self.hears_stems() || !matches!(heard, Heard::Stems)
    }
}

/// What a song is heard by: its reference's preview, its reference, or
/// its stems.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Heard {
    Preview,
    Reference,
    Stems,
}

/// One step of what a page brings in behind the song on screen
/// ([`warm_order`]): a song (by its place counted from the one on screen)
/// heard by something, or that song switched to its stems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarmStep {
    /// Bring all of what song `song` is heard by in.
    Warm { song: usize, heard: Heard },
    /// Start the song's stems coming in to take over from its reference
    /// where it is playing.
    HearStems { song: usize },
}

/// The order a page brings a set in behind the song on screen, `songs[0]`
/// (the rest in set order from it; `true` for a song with a reference):
/// every song's preview, so the whole set is playable almost at once; the
/// song on screen's reference, then its stems — taking over from the
/// reference once they are where it is; every other song's reference; then
/// every other song's stems. Proxies only, always: the originals are not
/// streamed to a page. Capped at [`Listen::Reference`], it stops at the
/// references: no stems, and no song switched to them.
#[must_use]
pub fn warm_order(songs: &[bool], listen: Listen) -> Vec<WarmStep> {
    let by_reference = |song: &usize| songs.get(*song).copied().unwrap_or(false);
    let rest = 1..songs.len();
    let mut steps: Vec<WarmStep> = (0..songs.len())
        .filter(by_reference)
        .map(|song| WarmStep::Warm {
            song,
            heard: Heard::Preview,
        })
        .collect();
    if !songs.is_empty() {
        if by_reference(&0) {
            steps.push(WarmStep::Warm {
                song: 0,
                heard: Heard::Reference,
            });
            steps.push(WarmStep::HearStems { song: 0 });
        }
        steps.push(WarmStep::Warm {
            song: 0,
            heard: Heard::Stems,
        });
    }
    steps.extend(
        rest.clone()
            .filter(by_reference)
            .map(|song| WarmStep::Warm {
                song,
                heard: Heard::Reference,
            }),
    );
    steps.extend(rest.map(|song| WarmStep::Warm {
        song,
        heard: Heard::Stems,
    }));
    steps.retain(|step| match *step {
        WarmStep::Warm { heard, .. } => listen.allows(heard),
        WarmStep::HearStems { .. } => listen.hears_stems(),
    });
    steps
}

#[cfg(test)]
mod warm_tests {
    use super::{Heard, Listen, WarmStep, warm_order};

    #[test]
    fn a_page_brings_every_preview_then_the_song_on_screen_then_the_rest() {
        use WarmStep::{HearStems, Warm};
        // On screen with a reference; then one without; then one with.
        assert_eq!(
            warm_order(&[true, false, true], Listen::Full),
            [
                Warm {
                    song: 0,
                    heard: Heard::Preview
                },
                Warm {
                    song: 2,
                    heard: Heard::Preview
                },
                Warm {
                    song: 0,
                    heard: Heard::Reference
                },
                HearStems { song: 0 },
                Warm {
                    song: 0,
                    heard: Heard::Stems
                },
                Warm {
                    song: 2,
                    heard: Heard::Reference
                },
                Warm {
                    song: 1,
                    heard: Heard::Stems
                },
                Warm {
                    song: 2,
                    heard: Heard::Stems
                },
            ]
        );
        assert_eq!(
            warm_order(&[false], Listen::Full),
            [Warm {
                song: 0,
                heard: Heard::Stems
            }]
        );
        assert_eq!(warm_order(&[], Listen::Full), Vec::<WarmStep>::new());
    }

    #[test]
    fn capped_at_reference_a_page_brings_the_references_and_never_a_stem() {
        use WarmStep::Warm;
        assert_eq!(
            warm_order(&[true, false, true], Listen::Reference),
            [
                Warm {
                    song: 0,
                    heard: Heard::Preview
                },
                Warm {
                    song: 2,
                    heard: Heard::Preview
                },
                Warm {
                    song: 0,
                    heard: Heard::Reference
                },
                Warm {
                    song: 2,
                    heard: Heard::Reference
                },
            ]
        );
        // A song with no reference brings nothing in: what it plays is live.
        assert_eq!(warm_order(&[false], Listen::Reference), []);
    }

    #[test]
    fn full_is_the_default_cap_and_the_order_before_it() {
        assert_eq!(Listen::default(), Listen::Full);
        for songs in [&[true, false, true][..], &[false], &[true], &[]] {
            assert!(
                warm_order(songs, Listen::default())
                    .iter()
                    .any(|s| matches!(
                        s,
                        WarmStep::Warm {
                            heard: Heard::Stems,
                            ..
                        }
                    ))
                    || songs.is_empty()
            );
        }
        assert!(!Listen::Reference.allows(Heard::Stems));
        assert!(Listen::Reference.allows(Heard::Reference));
        assert!(Listen::Reference.allows(Heard::Preview));
        assert!(Listen::Full.allows(Heard::Stems));
    }
}
