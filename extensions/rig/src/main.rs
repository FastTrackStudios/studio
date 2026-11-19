//! `rig-extension`: Signal's rig engine, served to the host that started it.

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    rig_extension::start_engine();
    architect_extensions::guest::Guest::extension(&rig_extension::RigEngine).serve()
}
