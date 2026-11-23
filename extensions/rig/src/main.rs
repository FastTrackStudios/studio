//! `rig-extension`: Signal's rig engine, served to the host that started it.

use architect_extensions_dioxus::guest::GuestExt as _;
fn main() -> Result<(), architect_extensions::guest::GuestError> {
    // To stderr, which the host keeps as the extension's log (EXT_LOGS).
    architect::host::init_tracing("info");
    rig_extension::start_engine();
    architect_extensions::guest::Guest::extension(&rig_extension::RigEngine).serve()
}
