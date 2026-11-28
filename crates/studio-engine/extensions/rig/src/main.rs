//! `rig-extension`: Signal's rig engine, served to the host that started it.
//!
//! It draws nothing, so it is served with no view runner: no Dioxus in the
//! engine's process (the promise in `boundaries.toml`).

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    // To stderr, which the host keeps as the extension's log (EXT_LOGS).
    architect::host::init_tracing("info");
    rig_extension::start_engine();
    architect_extensions::guest::Guest::extension_with(&rig_extension::RigEngine, &|_| None).serve()
}
