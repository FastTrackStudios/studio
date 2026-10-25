//! The rig libraries as books: each rig's library one Loro doc in a
//! `.session` file beside its old styx directory (`signal_rig_host::book`,
//! ADR 0002), so it syncs between the account's devices.
//!
//! Turned on once, where the rigs run — the engine process on a desk, the
//! app itself on an iPad — before the first rig makes its home. A run that
//! must leave no mark (design, ephemeral) keeps the files as they are.

/// Turn books on for this process's rigs. Signed in, an account's sync
/// carries them, so a fresh book waits for the account's library before
/// writing the shipped defaults into it.
pub fn enable() {
    if guitar::library::rig_is_ephemeral() {
        tracing::info!(
            rig.books = "off",
            "rig libraries stay files: this run leaves no mark"
        );
        return;
    }
    let syncs = fts_account::signed_in().is_some();
    signal_rig_host::book::enable(syncs);
    tracing::info!(
        rig.books = "on",
        rig.books_sync = syncs,
        "rig libraries are books"
    );
}
