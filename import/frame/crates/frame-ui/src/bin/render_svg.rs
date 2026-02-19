#[cfg(not(feature = "svg_export"))]
fn main() {
    eprintln!("render_svg requires --features svg_export");
    std::process::exit(2);
}

#[cfg(feature = "svg_export")]
fn main() {
    if let Err(err) = frame_ui::svg::run_cli() {
        eprintln!("render_svg error: {err}");
        std::process::exit(1);
    }
}
