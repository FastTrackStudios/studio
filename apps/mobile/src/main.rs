//! Studio on an iPhone or an iPad: Architect's iOS app
//! (`architect_mobile::run`) with Studio's distribution, the one the
//! desktop app and studio-web run (`studio_extensions::distribution`).

#[cfg(target_os = "ios")]
fn main() -> std::process::ExitCode {
    architect_mobile::run(studio_extensions::distribution())
}

/// Not an Apple phone or tablet: nothing to run (build with
/// `--target aarch64-apple-ios` or `aarch64-apple-ios-sim`).
#[cfg(not(target_os = "ios"))]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::FAILURE
}
