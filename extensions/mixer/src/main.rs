//! `studio-mixer`: the mixer's process.

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    architect_extensions::guest::Guest::extension(&studio_mixer::Mixer).serve()
}
