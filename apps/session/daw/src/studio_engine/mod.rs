//! The engine side of Session: what runs whether or not a window is open.
//!
//! Studio is splitting its engine from its GUI (`docs/design/engine-boundary.md`,
//! `docs/research/engine-process-split.md`): the `studio` CLI runs the whole
//! engine (`studio engine`), and a window is a remote control of it. This
//! module is where the engine side goes as it leaves the window's code, and
//! the seam the crate `studio-engine` will be cut along once what is here no
//! longer reaches back into the window's modules (`session-daw` is still one
//! strongly connected component — `docs/design/session-daw-split/`).
//!
//! What is here:
//!
//! - [`set`]: the set the engine has open — its songs, the current one, the
//!   ones still arriving, an open under way — owned by the engine and
//!   published to every window, the CLI and a model alike
//!   (`docs/design/set-engine.md` step 3).
//! - [`set_actions`]: the set's actions on the Action List (`SESSION_SONGS`,
//!   `SESSION_SONG`, `SESSION_OPEN_SETLIST`, …), run against [`set`] — no
//!   window needed.
//! - [`mcp`]: the Action List served to a model and to the CLI on
//!   127.0.0.1:47811.
//! - [`headless`]: standing the engine up with no window — what `studio
//!   engine` runs.
//!
//! The rule for code here: nothing in it draws, and nothing in it waits for
//! a window to do something. A window reads [`set::subscribe`] and asks
//! [`set`]'s operations; it never holds the set itself.

pub mod headless;
pub mod mcp;
pub mod set;
pub mod set_actions;
