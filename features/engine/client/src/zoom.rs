//! Zoom commands ([`daw_view::zoom`]: what a view's zoom keys ask, the
//! arithmetic that frames them), and the history of views a panel has
//! framed, which walks them with the view kit's timeline.

pub use daw_view::zoom::*;

/// The zooms already been to, for `z u` / `z r` and the toggles:
/// [`timeline::zoom::ZoomHistory`] over the panel's [`Target`]s.
#[derive(Debug, Default)]
pub struct History(timeline::zoom::ZoomHistory<Target>);

impl History {
    /// Where to go for `request` from `now`, remembering where it was.
    /// `None` when there is nowhere to go.
    pub fn go(
        &mut self,
        now: Target,
        request: Request,
        framed: impl Fn() -> Target,
    ) -> Option<Target> {
        match request {
            Request::Back => self.0.back(now),
            Request::Forward => self.0.forward(now),
            Request::Frame { toggle, .. } => self.0.frame(now, toggle, |t| t.near(&now), framed),
            Request::ScrollBy { .. } | Request::Shape { .. } | Request::Pinch { .. } => {
                Some(framed())
            }
            Request::Open { .. } | Request::Scale { .. } => {
                self.0.forget_toggle();
                Some(framed())
            }
        }
    }
}
