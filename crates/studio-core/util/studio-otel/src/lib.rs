//! Studio's telemetry on plain `tracing` + OpenTelemetry: what every Studio
//! process (the app's window, the engine, its render workers) exports, and
//! what a browser page says about itself.
//!
//! - [`resource_attributes`]: which build sent a span: `service.name`,
//!   `service.version`, `deployment.environment`, `studio.platform`
//!   (`linux`, `macos`, `web`, …) and `host.arch`. Natively they are the
//!   exporter's resource; a page puts them on the OTLP/JSON it posts.
//! - [`Endpoint`]: where spans go. The process's own
//!   `OTEL_EXPORTER_OTLP_*` first (a developer's local collector), else
//!   the collector a release baked in (`STUDIO_OTLP_ENDPOINT` and its
//!   ingest token `STUDIO_OTLP_TOKEN`, from the release workflow), else
//!   nowhere: a plain build sends nothing.
//! - [`layer`] (native): the `tracing` layer exporting every span, with the
//!   global meter's metrics beside it.
//! - [`set`]: a field added to the current span's exported
//!   attributes, for a span opened elsewhere (the wide event, enriched
//!   where the value is known); nothing in a page.
//! - [`install_crash_hook`] (native): a panic becomes a `crash` span (the
//!   wide event: thread, where, what), exported before the hook that was
//!   there runs and the process goes.
//! - [`Crash`]: a panic as those fields, on every target (the page builds
//!   its own crash span from it).

#[cfg(not(target_arch = "wasm32"))]
mod export;
#[cfg(not(target_arch = "wasm32"))]
pub use export::{adopt, flush, install_crash_hook, layer, layer_to, set};

/// [`set`]'s browser form: a page exports no `tracing` spans (it posts its
/// own OTLP/JSON), so there is no span attribute to add.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::needless_pass_by_value)]
pub fn set<K, V>(_key: K, _value: V) {}

/// The collector a release build bakes in (`STUDIO_OTLP_ENDPOINT` at build
/// time, through `build.rs`: the OTLP base URL, `/v1/traces` and
/// `/v1/metrics` under it), empty otherwise.
pub const SHIPPED_ENDPOINT: &str = env!("STUDIO_OTLP_ENDPOINT_BAKED");

/// The shipped collector's ingest token (`STUDIO_OTLP_TOKEN`): a static
/// bearer the collector accepts for OTLP and nothing else, since the
/// exporter starts before anyone signs in.
const SHIPPED_TOKEN: &str = env!("STUDIO_OTLP_TOKEN_BAKED");

/// Where a process's spans go.
#[derive(Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// `OTEL_EXPORTER_OTLP_ENDPOINT` (or its `_TRACES_` form) is set: the
    /// exporter reads the standard variables itself.
    Environment,
    /// The collector a release baked in.
    Shipped {
        /// The OTLP base URL, no trailing `/`.
        base: String,
        /// Its ingest bearer, if it takes one.
        token: Option<String>,
    },
}

impl std::fmt::Debug for Endpoint {
    // The token is a credential: never in a log line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Environment => f.write_str("Environment"),
            Self::Shipped { base, token } => f
                .debug_struct("Shipped")
                .field("base", base)
                .field("token", &token.as_ref().map(|_| "<set>"))
                .finish(),
        }
    }
}

impl Endpoint {
    /// This process's: its environment, else what the build baked in.
    #[must_use]
    pub fn resolve() -> Option<Self> {
        Self::choose(
            |var| std::env::var(var).ok(),
            Some(SHIPPED_ENDPOINT),
            Some(SHIPPED_TOKEN),
        )
    }

    /// From `env` (a variable's value), else `shipped` and its `token`.
    /// Blank values count as unset.
    #[must_use]
    pub fn choose(
        env: impl Fn(&str) -> Option<String>,
        shipped: Option<&str>,
        token: Option<&str>,
    ) -> Option<Self> {
        let named = |var: &str| env(var).is_some_and(|v| !v.trim().is_empty());
        if named("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT") || named("OTEL_EXPORTER_OTLP_ENDPOINT") {
            return Some(Self::Environment);
        }
        let base = shipped.map(str::trim).filter(|b| !b.is_empty())?;
        Some(Self::Shipped {
            base: base.trim_end_matches('/').to_owned(),
            token: token
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned),
        })
    }
}

/// `production` in a release build, `development` otherwise.
#[must_use]
pub const fn environment() -> &'static str {
    if cfg!(debug_assertions) {
        "development"
    } else {
        "production"
    }
}

/// The platform a build runs on: the OS (`linux`, `macos`, `ios`), or `web`
/// in a browser.
#[must_use]
pub const fn platform() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web"
    } else {
        std::env::consts::OS
    }
}

/// What every span `service` exports says about the build that sent it.
#[must_use]
pub fn resource_attributes(service: &str, version: &str) -> Vec<(&'static str, String)> {
    vec![
        ("service.name", service.to_owned()),
        ("service.version", version.to_owned()),
        ("deployment.environment", environment().to_owned()),
        ("studio.platform", platform().to_owned()),
        ("host.arch", std::env::consts::ARCH.to_owned()),
    ]
}

/// A panic, as the crash span's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crash {
    /// The thread that panicked (`<unnamed>` when it has no name).
    pub thread: String,
    /// `file:line:column` of the panic, when known.
    pub at: String,
    /// The panic's message.
    pub message: String,
}

impl Crash {
    /// The panic `info` describes, on the current thread.
    #[must_use]
    pub fn of(info: &std::panic::PanicHookInfo<'_>) -> Self {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "a panic without a message".to_owned());
        Self {
            thread: std::thread::current()
                .name()
                .unwrap_or("<unnamed>")
                .to_owned(),
            at: info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_default(),
            message,
        }
    }

    /// Record it: a `crash` span (status error), closed before this
    /// returns, with a `panic` line logged inside it for the process's own
    /// log (its fields are the span's).
    pub fn record(&self) {
        let span = tracing::error_span!(
            "crash",
            crash.thread = %self.thread,
            crash.at = %self.at,
            crash.message = %self.message,
            otel.status_code = "ERROR",
            otel.status_message = %self.message,
        );
        let _in = span.enter();
        tracing::error!("panic");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |var| {
            vars.iter()
                .find(|(k, _)| *k == var)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn the_environment_wins_over_the_shipped_collector() {
        for var in [
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
        ] {
            let got = Endpoint::choose(
                env(&[(var, "http://127.0.0.1:4318")]),
                Some("https://collector.example/otlp"),
                Some("t"),
            );
            assert_eq!(got, Some(Endpoint::Environment), "{var}");
        }
    }

    #[test]
    fn a_release_sends_to_the_collector_it_baked_in() {
        let got = Endpoint::choose(
            env(&[("OTEL_EXPORTER_OTLP_ENDPOINT", "  ")]),
            Some("https://collector.example/otlp/"),
            Some(" secret "),
        );
        assert_eq!(
            got,
            Some(Endpoint::Shipped {
                base: "https://collector.example/otlp".into(),
                token: Some("secret".into()),
            })
        );
        let no_token = Endpoint::choose(env(&[]), Some("https://c.example"), Some(""));
        assert_eq!(
            no_token,
            Some(Endpoint::Shipped {
                base: "https://c.example".into(),
                token: None
            })
        );
    }

    #[test]
    fn a_plain_build_sends_nothing() {
        assert_eq!(Endpoint::choose(env(&[]), None, Some("t")), None);
        assert_eq!(Endpoint::choose(env(&[]), Some(" "), None), None);
    }

    #[test]
    fn the_token_never_reaches_a_log_line() {
        let shipped = Endpoint::Shipped {
            base: "https://c.example".into(),
            token: Some("hunter2".into()),
        };
        let shown = format!("{shipped:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("<set>"), "{shown}");
    }

    #[test]
    fn a_span_says_which_build_and_platform_sent_it() {
        let attrs = resource_attributes("studio-app", "26.11.3");
        let get = |k: &str| {
            attrs
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("service.name"), Some("studio-app"));
        assert_eq!(get("service.version"), Some("26.11.3"));
        assert_eq!(get("studio.platform"), Some(std::env::consts::OS));
        assert_eq!(get("host.arch"), Some(std::env::consts::ARCH));
        assert_eq!(get("deployment.environment"), Some(environment()));
    }
}
