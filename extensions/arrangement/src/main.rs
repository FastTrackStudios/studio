//! `studio-arrangement`: Studio's arrangement view, served to the host that
//! started it.

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    // The mixer's window actions are the mixer pane's (`studio.mixer`
    // claims them): a key here that runs one goes to the engine, which
    // asks that pane.
    session_engine::window_queue::hand_over(&studio_view_link::MIXER_ACTIONS);
    studio_arrangement::attach();
    architect_extensions::guest::Guest::extension(&studio_arrangement::Arrangement).serve()
}
