//! The shipped collector, from the environment into the crate.
//!
//! A release build exports `STUDIO_OTLP_ENDPOINT` and `STUDIO_OTLP_TOKEN`
//! (`.github/workflows/desktop.yml`, from the organization's OTLP variable
//! and secret). Through a build script rather than `option_env!`, as
//! fts-version does its version: the values are then build-script output,
//! which cargo reruns on (`rerun-if-env-changed`) and kache keys on, so a
//! cached plain build is never replayed into a release. Unset, both are
//! empty and the build sends nothing.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for var in ["STUDIO_OTLP_ENDPOINT", "STUDIO_OTLP_TOKEN"] {
        println!("cargo:rerun-if-env-changed={var}");
        let value = std::env::var(var).unwrap_or_default();
        println!("cargo:rustc-env={var}_BAKED={}", value.trim());
    }
}
