//! Write the painters' mixer strip out as a theme: REAPER's layout to the
//! pixel, the reference the native strip is checked against.
//!
//! ```sh
//! cargo run -r -p daw-view-tools --bin theme_export              # to features/engine/view-kit/themes/fts-dark.frame
//! cargo run -r -p daw-view-tools --bin theme_export -- out.frame # elsewhere
//! ```
//!
//! The console theme (`features/engine/view-kit/themes/console/`) is not written
//! here: it is its files, edited as they are. See
//! `daw_view_kit::skin::export`.

fn main() {
    // The editor's `editor.*` tokens, as the app writes them.
    midi_editor::install();
    let palette = daw_view_kit::palette::Palette::from_theme(&daw_ui::theming::Theme::dark());
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "features/engine/view-kit/themes/fts-dark.frame".to_owned());
    let themes = vec![(out, daw_view_kit::skin::export::fts_dark(&palette))];
    for (out, markup) in themes {
        // Compiled before it is written: a theme the mixer would refuse is
        // not worth overwriting a good one with.
        if let Err(errors) =
            frame::theme::runtime::Theme::from_markup(&markup, daw_view_kit::skin::schema(), None)
        {
            eprintln!("{out}:");
            for e in errors.iter().take(20) {
                eprintln!("  {e}");
            }
            std::process::exit(1);
        }
        std::fs::write(&out, &markup).unwrap_or_else(|e| panic!("{out}: {e}"));
        println!("wrote {out} ({} lines)", markup.lines().count());
    }
}
