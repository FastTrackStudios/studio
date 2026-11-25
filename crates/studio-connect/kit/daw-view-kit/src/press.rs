//! A control's named action, run on the list this build has
//! (`docs/design/actions-everywhere.md`): the desktop's, through the window
//! (`crate::window::press`), or the page's (the engine's body's `web_run`).

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
        let id = id.to_owned();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(error) = crate::body::body()
                .web_run(id.clone(), args.into_bytes())
                .await
            {
                tracing::warn!(action.id = %id, %error, "page: a control's action was refused");
            }
        });
        None
    }
    #[cfg(not(any(feature = "native", feature = "web")))]
    {
        let _ = (id, args);
        None
    }
}
