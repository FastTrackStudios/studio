//! A control's named action, run on the list this build has
//! (`docs/design/actions-everywhere.md`): the desktop's, through the window
//! (`crate::window::press`), or the page's (`session_engine::web_actions`).

/// Run `id` with `args` as a control does. What it said (JSON) where the
/// answer is at hand — the desktop's, run now — else `None`: the page's
/// runs in the background (its facade answers in time), and a refusal is
/// said once wherever it runs.
pub fn press(id: &str, args: &serde_json::Value) -> Option<serde_json::Value> {
    #[cfg(feature = "native")]
    {
        crate::window::press(id, args)
    }
    #[cfg(all(feature = "web", not(feature = "native")))]
    {
        let args = if args.is_null() {
            String::new()
        } else {
            args.to_string()
        };
        session_engine::web_actions::press(id, args);
        None
    }
    #[cfg(not(any(feature = "native", feature = "web")))]
    {
        let _ = (id, args);
        None
    }
}

/// Put the window's view actions on the page's list, once.
#[cfg(all(feature = "web", not(feature = "native")))]
pub fn register_page_actions() {
    thread_local! {
        static DONE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    if let Err(error) = session_engine::web_actions::extend(crate::view_actions::register) {
        tracing::error!(%error, "page: the window's actions stopped registering");
    }
}
