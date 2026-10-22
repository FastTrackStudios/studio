//! Signal domain in-process REAPER extension.
//!
//! Loaded directly by REAPER from `UserPlugins/`. Puts Signal's command
//! core (`signal::commands`, one command per `signal::actions::SignalActions`
//! id) on an architect `ActionList` run against this REAPER ([`commands`]),
//! registers every id on that list as a REAPER named command, and runs a
//! trigger by running its id on the list.
//!
//! Migrated off the old SHM-guest model (commit 7a6e470 in daw repo). The
//! `daw_extension_runtime::connect()` / `GuestOptions` API no longer
//! exists; we now use the in-process `ExtensionRuntime::new(context)`
//! pattern shared with session-extension and sync-extension.

pub mod commands;
pub mod daw_compat;
pub mod demo_profile;
pub mod demo_rig;
pub mod demo_setlist;
pub mod macro_learn;
pub mod place_switch;
pub mod scene_midi;

use std::cell::OnceCell;
use std::error::Error;

use architect::action::ActionList;
use daw_extension_runtime::ExtensionRuntime;
use fragile::Fragile;
use reaper_low::PluginContext;
use reaper_macros::reaper_extension_plugin;
use tracing::{error, info};

thread_local! {
    static APP: OnceCell<Fragile<SignalExtension>> = const { OnceCell::new() };
}

struct SignalExtension {
    runtime: ExtensionRuntime,
}

impl SignalExtension {
    fn new(context: PluginContext) -> eyre::Result<Self> {
        let runtime = ExtensionRuntime::new(context)?;
        let daw = runtime.build_daw()?;

        runtime.spawn(async move {
            // Health beacons for tests.
            let pid = std::process::id();
            let _ = daw
                .ext_state()
                .set("FTS_SIGNAL_EXT", "status", "ready", false)
                .await;
            let _ = daw
                .ext_state()
                .set("FTS_SIGNAL_EXT", "pid", &pid.to_string(), false)
                .await;

            // Signal's commands, on the list every trigger runs through.
            let list = match commands::list(daw.clone()) {
                Ok(list) => list,
                Err(e) => {
                    error!("signal command list failed: {e}");
                    return;
                }
            };

            // Each id on the list as a REAPER named command.
            let registry = daw.action_registry();
            let defs = list.infos();
            let total = defs.len();
            let mut registered = 0usize;
            for def in defs {
                let cmd_name = def.id.as_str();
                match registry.register(cmd_name, &def.display_name).await {
                    Ok(cmd_id) if cmd_id > 0 => {
                        registered += 1;
                    }
                    Ok(_) => {
                        tracing::warn!("signal action returned cmd_id=0: {cmd_name}");
                    }
                    Err(e) => {
                        error!("signal action register failed {cmd_name}: {e:#}");
                    }
                }
            }
            info!("signal-extension registered {registered}/{total} actions");

            // TODO(daw-track-api): `ActionRegistry::subscribe_actions` was removed
            // from daw main; there is no action-event stream to drive the trigger
            // loop. Actions are still registered above, but their triggers are not
            // handled until daw restores a subscription API; then each trigger is
            // `handle_action(&list, name)`.
            let _ = &list;
            error!(
                "signal-extension: action event subscription unavailable on daw main \
                 (subscribe_actions removed) — {registered}/{total} actions registered, \
                 but trigger handling is DISABLED (TODO daw-track-api)"
            );
        });

        Ok(Self { runtime })
    }

    fn timer(&self) {
        self.runtime.process_tasks();
    }
}

extern "C" fn timer_callback() {
    APP.with(|cell| {
        if let Some(app) = cell.get() {
            app.get().timer();
        }
    });
}

#[reaper_extension_plugin]
fn plugin_main(context: PluginContext) -> Result<(), Box<dyn Error>> {
    init_tracing();
    info!("signal-extension starting");

    let app = SignalExtension::new(context)?;
    app.runtime.add_timer(timer_callback)?;

    let stored = APP.with(|cell| cell.set(Fragile::new(app)).is_ok());
    if !stored {
        return Err("signal-extension already initialized".into());
    }

    info!("signal-extension loaded");
    Ok(())
}

fn init_tracing() {
    let Ok(log_file) = std::fs::File::create("/tmp/signal-extension.log") else {
        return;
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

/// Run a triggered REAPER named command: its id on Signal's command list.
#[expect(dead_code)]
async fn handle_action(list: &ActionList, command_name: &str) {
    info!("signal action triggered: {command_name}");
    if let Err(e) = commands::dispatch(list, command_name).await {
        error!("signal action {command_name}: {e}");
    }
}
