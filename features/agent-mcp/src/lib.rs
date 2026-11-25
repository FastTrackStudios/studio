//! Studio's MCP for an agent (`studio-mcp`).
//!
//! A stdio MCP server a client (Claude Code's `.mcp.json`) starts, which
//! reaches the running Studio through its control socket
//! (`<runtime dir>/fts/studio.sock`, the same path every launch, this
//! user only, no token): it needs no port and no secret, and it carries
//! on across Studio's restarts — a call made while Studio is down waits
//! for it to come back.
//!
//! What an agent gets on top of the Action List (`apps/studio/docs/mcp.md`):
//!
//! - **Curated, typed tools** named for what they do (`mix_state`,
//!   `listen`, `block_measure`, `balance`, `signal_block_set`, …), each
//!   with its action's own argument schema, beside the generic `actions`,
//!   `describe`, `run`, `batch` and `watch` for the other ~550.
//! - **Compact answers** ([`shape`]): a summary by default, `detail`
//!   levels, and pages instead of a cut ([`page`]).
//! - **Jobs** ([`jobs`]): a call that outlives its wait answers a job id
//!   at once, and `job` waits for its result.
//! - **Scripts** ([`script`]): JavaScript that chains many calls — loops,
//!   conditions, renders side by side — in one tool call.

pub mod jobs;
pub mod page;
pub mod script;
pub mod server;
pub mod shape;
pub mod tools;
pub mod upstream;

pub use server::{Bridge, Config};
