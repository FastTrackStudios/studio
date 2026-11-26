//! The app's own telemetry, natively: one plain `tracing` subscriber (the
//! engine's way, `engine-core`'s `telemetry`), logging to stderr and
//! exporting every span over OTLP when a collector is named: the
//! process's `OTEL_EXPORTER_OTLP_*`, else the one a release baked in
//! (`studio_otel::Endpoint`). A panic becomes a `crash` span, flushed
//! before the process goes.
//!
//! `RUST_LOG` overrides the default filter.

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// The app's service name in the collector.
const SERVICE: &str = "studio-app";

/// What the app logs unless `RUST_LOG` says otherwise. The app's own spans
/// are under its program's name (`Studio`, the package's [[bin]]), not the
/// package's: filtered as `studio_app`, none of them reached the collector
/// (the boot, the engine's, a song that did not open).
const DEFAULT_FILTER: &str = "warn,Studio=info,drive_dioxus=info";

/// Install the subscriber and the crash hook: once, first thing in `main`,
/// after the running version is set (`utils::version`).
pub fn init() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| DEFAULT_FILTER.into());
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(studio_otel::layer(SERVICE, utils::version::current()))
        .try_init();
    studio_otel::install_crash_hook();
}

/// `--crash-test`: crash on purpose, once telemetry is up, so a build's
/// crash report can be seen reaching its collector.
pub fn crash_test() -> ! {
    // A panic, not an abort: what a bug does, through the crash hook.
    #[allow(clippy::panic)]
    {
        panic!("studio --crash-test: a crash on purpose");
    }
}
