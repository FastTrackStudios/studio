//! The Organize toolbar's buttons. Placing what they insert on the song is
//! the engine's (`studio_engine::organize_edit`).

use session::section_kinds::{MarkerKind, SectionKind};

/// One button of the toolbar — what [`insert`] puts on the timeline.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Button {
    Marker(MarkerKind),
    Section(SectionKind),
    TimeSig(i32, i32),
}

impl Button {
    /// Every marker and section button.
    pub const NAMED: [Self; 22] = [
        Self::Marker(MarkerKind::CountIn),
        Self::Marker(MarkerKind::Start),
        Self::Marker(MarkerKind::End),
        Self::Marker(MarkerKind::SongStart),
        Self::Marker(MarkerKind::SongEnd),
        Self::Section(SectionKind::Intro),
        Self::Section(SectionKind::Verse),
        Self::Section(SectionKind::PreChorus),
        Self::Section(SectionKind::Chorus),
        Self::Section(SectionKind::Bridge),
        Self::Section(SectionKind::Outro),
        Self::Section(SectionKind::Instrumental),
        Self::Section(SectionKind::Solo),
        Self::Section(SectionKind::Hits),
        Self::Section(SectionKind::Interlude),
        Self::Section(SectionKind::Breakdown),
        Self::Section(SectionKind::Vamp),
        Self::Section(SectionKind::Refrain),
        Self::Section(SectionKind::Tag),
        Self::Section(SectionKind::Turnaround),
        Self::Section(SectionKind::CountIn),
        Self::Section(SectionKind::End),
    ];

    /// What a window names the button by (`ORGANIZE_PRESS`): `Marker(SongEnd)`,
    /// `Section(Chorus)`, `TimeSig(3, 4)`.
    #[must_use]
    pub fn key(self) -> String {
        format!("{self:?}")
    }

    /// The button `key` names ([`Self::key`]).
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        if let Some(sig) = key
            .strip_prefix("TimeSig(")
            .and_then(|k| k.strip_suffix(')'))
        {
            let (num, den) = sig.split_once(',')?;
            return Some(Self::TimeSig(
                num.trim().parse().ok()?,
                den.trim().parse().ok()?,
            ));
        }
        Self::NAMED.into_iter().find(|b| b.key() == key)
    }
}
