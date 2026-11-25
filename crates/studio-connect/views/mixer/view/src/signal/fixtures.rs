//! Real chains to build against until the engine answers `SIGNAL_CHAIN`.
//!
//! The Unbreakable kit's Kick folder, its seven Signal instances as
//! `signal.chain/1` (`fixtures/kick_kit.json`, made from the kit's own
//! states by `fixtures/kick_kit.py`).

use super::chain::{Chain, ChainError};

/// The Kick folder's chains, as JSON.
pub const KICK_KIT: &str = include_str!("fixtures/kick_kit.json");

/// Each Kick-folder track's path in the kit (`Drums/Kick/Sum/In`) and its
/// chain, in the kit's order.
///
/// # Errors
/// The fixture is not a list of `signal.chain/1` documents.
pub fn kick_kit() -> Result<Vec<(String, Chain)>, ChainError> {
    let doc: serde_json::Value =
        serde_json::from_str(KICK_KIT).map_err(|e| ChainError::Json(e.to_string()))?;
    doc.get("tracks")
        .and_then(serde_json::Value::as_array)
        .ok_or(ChainError::Shape("tracks"))?
        .iter()
        .map(|t| {
            let path = t
                .get("path")
                .and_then(serde_json::Value::as_str)
                .ok_or(ChainError::Shape("a track's path"))?;
            let chain = Chain::from_value(t.get("chain").ok_or(ChainError::Shape("a chain"))?)?;
            Ok((path.to_owned(), chain))
        })
        .collect()
}
