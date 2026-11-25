//! `studio-rig-status`: served to the host that started it.

use architect_extensions_dioxus::guest::GuestExt as _;
fn main() -> Result<(), architect_extensions::guest::GuestError> {
    architect_extensions::guest::Guest::extension(&studio_rig_status::Status).serve()
}
