#[cfg(feature = "svg_export")]
mod cli;
#[cfg(feature = "svg_export")]
mod context;
#[cfg(feature = "svg_export")]
mod effects;
#[cfg(feature = "svg_export")]
mod normalize;
#[cfg(feature = "svg_export")]
mod render;
#[cfg(feature = "svg_export")]
mod source_bridge;

#[cfg(feature = "svg_export")]
pub use cli::run_cli;
