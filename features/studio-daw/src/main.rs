//! Studio's DAW as a process: the same `Daw` Studio Architect links in,
//! served over vox to the Architect app that started it
//! (`extension.toml`'s `[entry]`). Its view draws Session's painted
//! widgets, which only a linked copy can put in the app's window today
//! (docs/design/studio-in-architect.md, the gaps).

use architect_extensions::guest::{Guest, GuestError};

fn main() -> Result<(), GuestError> {
    Guest::extension(&studio_daw::ext::Daw).serve()
}
