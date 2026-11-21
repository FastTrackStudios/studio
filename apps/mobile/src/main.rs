//! Studio on an iPhone or an iPad: Architect's iOS app
//! (`architect_mobile::run`) with Studio's distribution, the one the
//! desktop app and studio-web run (`studio_extensions::distribution`).

#[cfg(target_os = "ios")]
fn main() -> std::process::ExitCode {
    use tracing_subscriber::layer::SubscriberExt as _;
    // The app's events on stderr: what `dx serve` shows of the app, and
    // Xcode's console (as studio-web's go to the browser's console).
    let filter = tracing_subscriber::filter::Targets::new()
        .with_default(tracing::Level::INFO)
        .with_target("loro_internal", tracing::Level::WARN)
        .with_target("loro", tracing::Level::WARN);
    let subscriber = tracing_subscriber::registry().with(filter).with(
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(false),
    );
    let _ = tracing::subscriber::set_global_default(subscriber);
    architect_mobile::run(studio_extensions::distribution())
}

/// Not an Apple phone or tablet: nothing to run (build with
/// `--target aarch64-apple-ios` or `aarch64-apple-ios-sim`).
#[cfg(not(target_os = "ios"))]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::FAILURE
}
