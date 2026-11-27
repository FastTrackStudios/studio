//! A room: an organization's lasting place to play together (ADR 0003,
//! `apps/studio/CONTEXT.md`).
//!
//! A room is reached by one link that never changes
//! (`https://fasttrackstudio.app/join/<room>`). Its **leader** loads a
//! setlist or a lone session and drives the transport; everyone else in it
//! follows, and keeps following when the leader loads something else,
//! without reconnecting.
//!
//! - [`record`]: the room as its organization keeps it, one file per room
//!   in the org's Rooms folder ([`Rooms`]), and how its link is found
//!   again. While a leader has it live, the file says where to reach them
//!   ([`Live`]); it keeps what was last loaded, which the link opens to
//!   practise while nobody leads ([`Room::opens`]).
//! - [`state`]: what the room shares while it is live, on live-doc: one
//!   Loro doc with what is loaded ([`Loaded`]), and its presence, which
//!   carries who is here (people, and the agents editing through a
//!   member's MCP) and the leader's transport. The doc also says where the
//!   leader shares what it loaded ([`Share`]), so everyone's edits to it
//!   merge.
//! - [`lead`]: the leader's transport, as the entry everyone follows
//!   ([`lead::entry`]); members follow it with session-sync's
//!   `TransportSync`, as a live session playing together does.
//! - `net` (feature `net`): the room over vox. The leader hosts the doc
//!   and presence ([`net::RoomHost`]); a member holds one connection to
//!   them ([`net::RoomPeer`]) for as long as it is in the room, from a
//!   desktop's engine or a browser's page alike ([`net::attach_to`]).
//! - `gate` (feature `net`, on a device): the leader's door ([`gate::Door`]), which
//!   admits a call only when it presents the room's [`Pass`]: knowing the
//!   link, or the leader's endpoint, is not enough to join.
//!
//! Engine-free: the engine's half (opening what is loaded, moving the
//! transport) is engine-collab's `room`.

pub mod lead;
pub mod record;
pub mod state;

// The leader's door: serving is a device's (a page never leads a room).
#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
pub mod gate;
#[cfg(feature = "net")]
pub mod net;

pub use record::{
    APP_JOIN_BASE, Error, Guest, Live, Lobby, Opens, Pass, Room, RoomId, Rooms, guest_name,
    link_of, parse_link,
};
pub use state::{Kind, Loaded, RoomDoc, Share};
