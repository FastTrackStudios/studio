//! Studio's Lyrics as a process: the same `Lyrics` Studio Architect links
//! in, served over vox to the Architect app that started it
//! (`extension.toml`'s `[entry]`).

use architect_extensions::guest::{Guest, GuestError};

fn main() -> Result<(), GuestError> {
    Guest::extension(&studio_lyrics::ext::Lyrics).serve()
}
