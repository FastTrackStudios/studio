//! `studio-tempo-map`: tempo mapping, served to the host that started it.

use architect_extensions_dioxus::guest::GuestExt as _;
fn main() -> Result<(), architect_extensions::guest::GuestError> {
    architect_extensions::guest::Guest::extension(&studio_tempo_map::TempoMap).serve()
}
