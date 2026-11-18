//! `studio-mixer`: Studio's mixer and track strip, served to the host that
//! started it.

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    studio_view_link::attach();
    architect_extensions::guest::Guest::extension(&studio_mixer::Mixer).serve()
}
