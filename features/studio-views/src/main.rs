//! The views extension as a process: the same `StudioViews` a build
//! compiles in, served over vox to the Architect app that started it
//! (`extension.toml`'s `[entry]`).

use architect_extensions::guest::{Guest, GuestError};

fn main() -> Result<(), GuestError> {
    Guest::extension(&studio_views::ext::StudioViews).serve()
}
