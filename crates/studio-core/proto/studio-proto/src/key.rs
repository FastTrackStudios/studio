//! Key changes — REAPER-facing action contract.
//!
//! Thirty actions, one per key: fifteen majors and fifteen minors,
//! spanning the circle of fifths in both directions so the enharmonics
//! are all reachable. C♯ major and D♭ major are separate actions
//! because they are separate keys — same pitches, different spelling, and
//! a chart says one or the other.
//!
//! Each drops a key change at the edit cursor on the KEY track (see
//! `crate::key`). None of them touch REAPER's own key signature, which
//! has no API at all — `bake_key_signatures` is the separate, explicit
//! step that makes key snap agree.

use daw_proto::DawResult;

/// Set the key at the edit cursor.
#[architect::actions(namespace = "FTS_SESSION_KEY")]
pub trait KeyActions {
    #[action(
        undo,
        description = "Set the key at the edit cursor to C major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_c_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to G major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_g_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to D major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_d_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to A major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_a_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to E major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_e_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to B major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_b_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to F♯ major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_f_sharp_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to C♯ major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_c_sharp_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to F major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_f_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to B♭ major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_b_flat_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to E♭ major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_e_flat_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to A♭ major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_a_flat_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to D♭ major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_d_flat_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to G♭ major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_g_flat_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to C♭ major",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_c_flat_major(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to A minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_a_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to E minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_e_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to B minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_b_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to F♯ minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_f_sharp_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to C♯ minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_c_sharp_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to G♯ minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_g_sharp_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to D♯ minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_d_sharp_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to A♯ minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_a_sharp_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to D minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_d_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to G minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_g_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to C minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_c_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to F minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_f_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to B♭ minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_b_flat_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to E♭ minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_e_flat_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Set the key at the edit cursor to A♭ minor",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn set_key_a_flat_minor(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Write the KEY track's changes into the project's key signatures, so the MIDI editor's key snap matches. Saves and reloads the project.",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn bake_key_signatures(&self) -> DawResult<()>;

    #[action(
        undo,
        description = "Remove every key change from the KEY track",
        category = "Session",
        group = "Key"
    )]
    /// # Errors
    ///
    /// Returns an error if the DAW is unavailable or the key change
    /// cannot be written at the edit cursor.
    fn clear_key_changes(&self) -> DawResult<()>;
}
