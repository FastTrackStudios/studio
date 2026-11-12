//! The Patchbay core as an Architect extension: one `linked::Extension`
//! (FastTrackStudios/studio#410).
//!
//! Linked into a distribution (Patchbay, Studio Architect), it adds the
//! `patchbay` view and the `PATCHBAY_*` actions. Both run over this
//! machine's one engine ([`engine`]): the first action or view that needs
//! it starts it, or attaches to the engine another app already serves
//! (`fts-patchbay`, `patchbay serve`, another Architect app). Nothing
//! starts while the host only reads what the extension declares
//! (`Builder::of` runs [`Extension::build`] for that too).
//!
//! The device families (`dante`, `coreaudio`, `antelope`, `yamaha`, the
//! `PipeWire` graph) are still inside the engine; each becomes its own
//! extension in the stack #410 lays out.

use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use architect_extensions::linked::{Builder, Extension};
use dioxus::prelude::*;
use serde_json::Value;

use crate::proto::PatchbayServiceClient;
use crate::server::{DEFAULT_ADDR, Engine, EngineOptions};
use crate::ui::{Dialer, EngineLink, PatchbayApp, PatchbayHandle, Shell};

/// The extension.
#[derive(Debug, Clone, Copy, Default)]
pub struct Patchbay;

impl Extension for Patchbay {
    fn manifest(&self) -> &'static str {
        include_str!("../extension.toml")
    }

    fn build(&self, ext: &mut Builder) {
        ext.view("patchbay", PatchbayView)
            .action("PATCHBAY_STATUS", |_| {
                call(|c| async move { crate::report::status(&c).await })
            })
            .action("PATCHBAY_HEALTH", |_| {
                call(|c| async move { json(crate::report::collect_health(&c).await) })
            })
            .action("PATCHBAY_NOW", |_| {
                call(|c| async move { json(c.host_overview().await) })
            })
            .action("PATCHBAY_DEVICE_LIST", |_| {
                call(|c| async move { json(c.list_devices().await) })
            })
            .action("PATCHBAY_DEVICE_SHOW", |args| async move {
                let id = args
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.trim().is_empty())
                    .ok_or("give `id`")?
                    .to_owned();
                call(|c| async move { json(c.device(id).await) }).await
            });
    }
}

/// This machine's engine: started here, or the one another app serves.
///
/// The port is `PATCHBAY_ADDR`, the saved `network.bind`, else
/// [`DEFAULT_ADDR`]. Once started the engine is kept for the life of the
/// process; a failed start is tried again by the next caller.
///
/// The first call blocks while the engine starts (or the probe of the
/// port answers), at most a few seconds.
///
/// # Errors
/// The port is held by something that is not a Patchbay engine, or the
/// engine could not start.
// The lock is held across the start on purpose: one start at a time.
#[allow(clippy::significant_drop_tightening)]
pub fn engine() -> Result<&'static Engine, String> {
    static ENGINE: Mutex<Option<&'static Engine>> = Mutex::new(None);
    let mut slot = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(engine) = *slot {
        return Ok(engine);
    }
    let mut opts = EngineOptions::new(EngineOptions::configured_addr(DEFAULT_ADDR));
    opts.web = std::env::var("PATCHBAY_WEB_DIST")
        .ok()
        .map(std::path::PathBuf::from)
        .filter(|dir| dir.join("index.html").is_file())
        .map(architect::host::WebBundle::Dir);
    let engine: &'static Engine = Box::leak(Box::new(
        Engine::start_or_attach(opts).map_err(|e| e.to_string())?,
    ));
    tracing::info!(mode = ?engine.mode(), "patchbay engine ready");
    *slot = Some(engine);
    Ok(engine)
}

/// Run `f` with the engine's RPC client, on its runtime.
async fn call<F, Fut>(f: F) -> Result<Value, String>
where
    F: FnOnce(PatchbayServiceClient) -> Fut,
    Fut: Future<Output = Result<Value, String>> + Send + 'static,
{
    engine()?.run(f).await?
}

/// A call's answer as JSON, its error as text: what `--json` prints.
fn json<T: serde::Serialize, E: std::fmt::Display>(r: Result<T, E>) -> Result<Value, String> {
    serde_json::to_value(r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// The `patchbay` view: [`PatchbayApp`] over this machine's engine, able
/// to switch to other machines' engines.
#[component]
fn PatchbayView() -> Element {
    let started = use_hook(|| {
        engine().map(|engine| {
            provide_context(shell(engine));
        })
    });
    match started {
        Ok(()) => rsx! { PatchbayApp {} },
        Err(e) => rsx! {
            div { style: "padding: 24px; display: flex; flex-direction: column; gap: 8px;",
                div { style: "font-weight: 600;", "Patchbay's engine did not start" }
                div { "{e}" }
            }
        },
    }
}

/// What the view's shell provides: home is this machine's engine, and
/// other machines are dialled on the engine's runtime.
fn shell(engine: &'static Engine) -> Shell {
    Shell {
        home: EngineLink {
            handle: PatchbayHandle(Arc::new(engine.client().clone())),
            stream: Rc::new(engine.stream_client().clone()),
        },
        dial: Some(Dialer(Rc::new(move |url: String| {
            Box::pin(async move {
                let (client, stream) = engine
                    .spawn(async move { crate::server::dial(&url).await })
                    .await??;
                Ok(EngineLink {
                    handle: PatchbayHandle(Arc::new(client)),
                    stream: Rc::new(stream),
                })
            })
        }))),
        on_home_lost: Callback::new(|()| {
            tracing::warn!("patchbay: the link to this machine's engine ended");
        }),
    }
}
