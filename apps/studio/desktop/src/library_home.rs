//! The library (`library::ui`) as the front screen's home. The library
//! knows nothing of sides or engines: this hands it the window's size in
//! points, kept current through a rotation, and turns what a press asks
//! for into the side it goes into (a part plays in Signal; a rehearsal or
//! a session opens in Session).

use dioxus::prelude::*;
use library::ui::{LibraryApp, Open, Opener, Viewport};

use crate::front::Side;

#[component]
pub fn LibraryHome(on_pick: EventHandler<Side>) -> Element {
    let window = dioxus_native::use_window();
    let mut size = use_context_provider(|| Viewport(Signal::new((1194.0, 834.0))));
    use_future(move || {
        let window = window.clone();
        async move {
            loop {
                let px = window.surface_size();
                if px.height > 0 {
                    let scale = window.scale_factor().max(0.1);
                    let pt = (
                        (f64::from(px.width) / scale) as f32,
                        (f64::from(px.height) / scale) as f32,
                    );
                    let was = *size.0.peek();
                    if (was.0 - pt.0).abs() > 1.0 || (was.1 - pt.1).abs() > 1.0 {
                        size.0.set(pt);
                    }
                }
                futures_timer::Delay::new(std::time::Duration::from_millis(300)).await;
            }
        }
    });
    use_context_provider(|| {
        Opener(Callback::new(move |what: Open| {
            match what {
                Open::Part(_) => on_pick.call(Side::Signal),
                #[cfg(feature = "session-app")]
                Open::Rehearse(_) | Open::Session(_) => on_pick.call(Side::Session),
                // A build without Session has nowhere to rehearse.
                #[cfg(not(feature = "session-app"))]
                Open::Rehearse(_) | Open::Session(_) => {}
            }
        }))
    });
    rsx! { LibraryApp {} }
}
