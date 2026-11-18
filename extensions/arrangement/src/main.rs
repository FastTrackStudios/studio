//! `studio-arrangement`: Studio's arrangement view, served to the host that
//! started it.

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    studio_arrangement::attach();
    architect_extensions::guest::Guest::extension(&studio_arrangement::Arrangement).serve()
}
