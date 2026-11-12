//! Session's song progress bar and transport controls, as one source for
//! every surface that draws them: Session's views (`session::ui::components`
//! re-exports them), and Studio's island on a share link's now-playing view
//! (studio#445), a WASI component. Dioxus only, no Session types.

pub mod progress;
pub mod transport;

pub use progress::*;
pub use transport::*;
