//! `daw-extension`: Studio's engine, served to the host that started it.

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    daw_extension::start_engine(std::env::var("FTS_DAW_OPEN").ok());
    architect_extensions::guest::Guest::extension(&daw_extension::Daw).serve()
}
