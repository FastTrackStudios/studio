//! The set the engine has open ([`set`]): its songs, the current one, the
//! ones still arriving, an open under way — owned by the engine and
//! published to every window, the CLI and a model alike. studio-engine's
//! `studio_engine` adds its openers, its actions, the MCP server, the
//! headless engine and the view host around it.

pub use engine_runtime::client::studio_engine::*;
pub mod set;
