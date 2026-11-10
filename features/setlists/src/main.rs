//! The setlists extension as a process: the same `Setlists` a build
//! compiles in, served over vox to the Architect app that started it
//! (`extension.toml`'s `[entry]`).

use architect_extensions::guest::{Guest, GuestError};

fn main() -> Result<(), GuestError> {
    Guest::extension(&setlists::ext::Setlists).serve()
}
