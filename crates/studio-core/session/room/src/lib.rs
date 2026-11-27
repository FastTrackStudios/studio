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
//!   ([`Live`]).
//! - [`state`]: what the room shares while it is live, on live-doc: one
//!   Loro doc with what is loaded ([`Loaded`]), and its presence, which
//!   carries who is here and the leader's transport.
//! - [`lead`]: the leader's transport, as the entry everyone follows
//!   ([`lead::entry`]); members follow it with session-sync's
//!   `TransportSync`, as a live session playing together does.
//! - `net` (feature `net`): the room over vox. The leader hosts the doc
//!   and presence ([`net::RoomHost`]); a member holds one connection to
//!   them ([`net::RoomPeer`]) for as long as it is in the room.
//!
//! Engine-free: the engine's half (opening what is loaded, moving the
//! transport) is engine-collab's `room`.

pub mod lead;
pub mod record;
pub mod state;

#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
pub mod net;

pub use record::{APP_JOIN_BASE, Error, Live, Room, RoomId, Rooms, link_of, parse_link};
pub use state::{Kind, Loaded, RoomDoc};
