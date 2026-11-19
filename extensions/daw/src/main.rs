//! `daw-extension`: Studio's engine, served to the host that started it.

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    daw_extension::start_engine(std::env::var("FTS_DAW_OPEN").ok());
    daw_extension::deck::lead();
    daw_extension::meters::publish();
    // Nothing in a song stays unsaved: every open song is written to its
    // file shortly after it changes.
    session_engine::autosave::start();
    architect_extensions::guest::Guest::extension(&daw_extension::Daw).serve()
}
