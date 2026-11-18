//! `studio-arrangement`: the arrangement's process.

fn main() -> Result<(), architect_extensions::guest::GuestError> {
    architect_extensions::guest::Guest::extension(&studio_arrangement::Arrangement).serve()
}
