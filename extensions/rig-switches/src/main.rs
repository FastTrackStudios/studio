//! `studio-rig-switches`: served to the host that started it.

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    architect_extensions::guest::Guest::extension(&studio_rig_switches::Switches).serve()
}
