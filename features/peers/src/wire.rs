//! What one Studio engine tells another: what it runs, and what it plays.

use facet::Facet;

/// What an engine runs now. It changes: a desk opens a set, an iPad
/// starts its keys rig.
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct Roles {
    /// It plays a set's tracks (Session's set engine has one open).
    pub tracks: bool,
    /// The rigs it runs (`guitar`, `keys`, …): the ones started.
    pub rigs: Vec<String>,
}

impl Roles {
    /// Says so in a few words: `tracks + guitar`, `guitar`, `idle`.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.tracks {
            parts.push("tracks".to_owned());
        }
        parts.extend(self.rigs.iter().cloned());
        if parts.is_empty() {
            "idle".to_owned()
        } else {
            parts.join(" + ")
        }
    }
}

/// One engine, as it describes itself to its peers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct About {
    /// What a person calls it ("Cody's iPad").
    pub name: String,
    /// `ios`, `macos`, `linux`, …
    pub platform: String,
    pub roles: Roles,
    /// Its build (`FTS_RELEASE_VERSION`, or the crate version).
    pub version: String,
    /// The endpoint it follows, if it follows one (empty: none).
    pub following: String,
}

/// What an engine plays now: the set and song up, the section, the
/// transport. The whole state each time, so a late follower is right
/// after one.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct NowPlaying {
    /// Bumped by each change the engine publishes.
    pub revision: u64,
    /// The set that is up (empty: none).
    pub setlist: String,
    /// The song that is up, by its title (empty: none), and its place in
    /// the set.
    pub song: String,
    pub song_index: u32,
    /// The section the playhead is in (empty: unknown).
    pub section: String,
    /// The rig cue the playhead is in (a guitar patch or scene the set
    /// names, `Sad But True - Main`), when the source knows it; empty: the
    /// follower looks it up in its own song's cues.
    #[facet(default)]
    pub cue: String,
    pub playing: bool,
    /// Seconds into the song, at `at_ms`.
    pub position: f64,
    pub bpm: f64,
    /// When this was true, ms since the epoch on the publisher's clock.
    pub at_ms: u64,
}

impl NowPlaying {
    /// Where the playhead is at `now_ms` (on the same clock as `at_ms`),
    /// moving on from `position` while it plays.
    #[must_use]
    pub fn position_at(&self, now_ms: u64) -> f64 {
        if !self.playing {
            return self.position;
        }
        let elapsed = now_ms.saturating_sub(self.at_ms);
        // ms of a performance: well inside f64's exact integers.
        #[allow(clippy::cast_precision_loss)]
        let secs = elapsed as f64 / 1000.0;
        self.position + secs
    }
}

#[cfg(feature = "vox")]
pub mod service {
    //! The one service a peer serves its account's other engines (behind
    //! the mesh's gate: admitted devices only).

    use super::{About, NowPlaying};

    #[architect::rpc]
    pub trait StudioPeer {
        /// What this engine is and runs now.
        fn about(&self) -> About;
        /// What it plays now.
        fn now(&self) -> NowPlaying;
        /// Each change of what it plays, the state now first.
        #[subscribe]
        fn playing(&self) -> NowPlaying;
    }
}

/// One of the account's other engines, as this one sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct PeerRow {
    pub endpoint: String,
    /// What the address book calls it.
    pub name: String,
    pub platform: String,
    /// It answered just now.
    pub reachable: bool,
    /// What it says of itself (default when it did not answer).
    pub about: About,
}

/// This engine on the mesh, for the CLI and a window.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct PeersStatus {
    /// Whether the mesh is up at all (signed in, endpoint bound).
    pub up: bool,
    /// Why not, when it is not.
    pub why: String,
    /// This engine's endpoint id.
    pub id: String,
    pub about: About,
    pub now: NowPlaying,
    pub peers: Vec<PeerRow>,
    /// The last account pass: `ok`, or what went wrong.
    pub account: String,
}

#[cfg(feature = "vox")]
pub mod control {
    //! The engine's own handle on its mesh: served on the engine's local
    //! router (never to peers), for `studio peers …` and for a window
    //! that plays a set in another process.

    use super::{NowPlaying, PeersStatus, RigCues, Roles};

    #[architect::rpc]
    pub trait PeersControl {
        /// The mesh and the peers on it, each asked what it runs.
        async fn status(&self) -> PeersStatus;
        /// Follow `peer` (an endpoint id, or a peer's name as the address
        /// book has it). Empty: what was done; else why not.
        async fn follow(&self, peer: String) -> String;
        async fn unfollow(&self);
        /// What a window in another process plays now: published as this
        /// engine's, and it runs tracks while it does.
        async fn publish(&self, now: NowPlaying);
        /// Roles a window adds (tracks) beside the engine's own rigs.
        async fn set_roles(&self, roles: Roles);
        /// The rig's cues for its songs, kept in its book (so they sync to
        /// the account's other engines). Empty: done; else why not.
        async fn set_cues(&self, cues: RigCues) -> String;
    }
}

/// One rig cue: a point in a song, and the patch or scene it names.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct Cue {
    /// Seconds into the song.
    pub at: f64,
    pub name: String,
}

/// A song's rig cues, in order.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct SongCues {
    pub song: String,
    pub cues: Vec<Cue>,
}

/// Every song's rig cues: what a rig book keeps under [`RIG_CUES`], so it
/// syncs with the rest of the rig.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct RigCues {
    pub songs: Vec<SongCues>,
}

/// Where a rig book keeps its [`RigCues`].
pub const RIG_CUES: &str = "cues.styx";

impl RigCues {
    /// `song`'s cues as the follow reads them (title matched as the
    /// follow matches titles).
    #[must_use]
    pub fn for_song(&self, song: &str) -> Vec<(f64, String)> {
        let titles: Vec<String> = self.songs.iter().map(|s| s.song.clone()).collect();
        crate::follow::find_song(&titles, song)
            .and_then(|i| self.songs.get(i))
            .map(|s| s.cues.iter().map(|c| (c.at, c.name.clone())).collect())
            .unwrap_or_default()
    }
}
