#[cfg(feature = "svg_export")]
pub fn run_cli() -> Result<(), String> {
    super::render::run_cli()
}
