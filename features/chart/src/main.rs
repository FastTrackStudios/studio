//! Studio's Chart as a process: the same `Chart` Studio Architect links
//! in, served over vox to the Architect app that started it
//! (`extension.toml`'s `[entry]`).

use architect_extensions::guest::{Guest, GuestError};

fn main() -> Result<(), GuestError> {
    Guest::extension(&studio_chart::ext::Chart).serve()
}
