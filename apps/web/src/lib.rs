//! Studio's web client.
//!
//! The app is Architect's web app, whole ([`architect_web::run`]), as a
//! library. What Studio adds is its distribution
//! (`studio_extensions`, with `web`), its views among it as one extension
//! (`studio.transport`): the setlist as a playlist, a
//! song's chart and lyrics, the lyrics, the arrangement and a signed-in
//! person's home. They are the island's components (`studio_island::view`),
//! compiled into this one wasm instead of fetched into frames: the
//! Architect server serves no Studio code, no island and no `/_ext` tree.
//! It stores the vaults, signs people in, enforces what each may read and
//! answers share links; this client asks it for content like any other.

/// Studio's distribution in a browser: [`studio_extensions::distribution`]
/// with `web` (the engine in the page, the set views the island's, the
/// window's extensions deferred).
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn distribution() -> architect_distribution::Distribution {
    studio_extensions::distribution()
}

/// The page's start: the app, with Studio's views.
#[cfg(target_arch = "wasm32")]
pub fn start() {
    use tracing_subscriber::layer::SubscriberExt as _;
    let filter = tracing_subscriber::filter::Targets::new()
        .with_default(tracing::Level::INFO)
        .with_target("loro_internal", tracing::Level::WARN)
        .with_target("loro", tracing::Level::WARN);
    let subscriber =
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_wasm::WASMLayer::new(
                tracing_wasm::WASMLayerConfigBuilder::new().build(),
            ));
    let _ = tracing::subscriber::set_global_default(subscriber);
    architect_web::run(distribution());
}
