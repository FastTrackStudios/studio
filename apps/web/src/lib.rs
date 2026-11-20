//! Studio's web client.
//!
//! The app is Architect's web app, whole ([`architect_web::run`]), as a
//! library. What Studio adds is its views, linked in as one extension
//! ([`StudioViews`], `studio.transport`): the setlist as a playlist, a
//! song's chart and lyrics, the lyrics, the arrangement and a signed-in
//! person's home. They are the island's components (`studio_island::view`),
//! compiled into this one wasm instead of fetched into frames: the
//! Architect server serves no Studio code, no island and no `/_ext` tree.
//! It stores the vaults, signs people in, enforces what each may read and
//! answers share links; this client asks it for content like any other.

/// The extension's manifest: its views, what claims they make (a
/// setlist note, a song note), what they read and a setlist's favourites.
pub const MANIFEST: &str = include_str!("../extension.toml");

#[cfg(target_arch = "wasm32")]
mod views;

#[cfg(target_arch = "wasm32")]
pub use views::StudioViews;

/// Studio's distribution in a browser ([`studio_extensions::distribution`],
/// with `stage`: in a browser it begins with Studio's engine in the page),
/// with the island's views for a setlist, a song and the lyrics linked in.
#[cfg(target_arch = "wasm32")]
#[must_use]
pub fn distribution() -> architect_distribution::Distribution {
    studio_extensions::distribution().linked(&StudioViews)
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

#[cfg(test)]
mod tests {
    #[test]
    fn the_manifest_reads_and_claims_setlists_and_songs() {
        let manifest: toml::Value = toml::from_str(super::MANIFEST).expect("the manifest is TOML");
        assert_eq!(manifest["id"].as_str(), Some("studio.transport"));
        let views: Vec<&str> = manifest["views"]
            .as_array()
            .expect("views")
            .iter()
            .filter_map(|v| v["id"].as_str())
            .collect();
        for view in ["setlist.playlist", "song", "lyrics", "arrangement", "home"] {
            assert!(views.contains(&view), "the manifest declares {view}");
        }
        let claims: Vec<String> = manifest["embeds"]
            .as_array()
            .expect("embeds")
            .iter()
            .flat_map(|e| {
                e["claims"]["note_type"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|t| t.as_str().map(str::to_owned))
            .collect();
        assert_eq!(claims, ["setlist", "song"]);
    }
}
