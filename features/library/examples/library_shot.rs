//! The library screens rendered to a picture: no window, no device, the
//! same Blitz paint path the app takes (`blitz_paint` into
//! `anyrender_vello`), at any size.
//!
//! ```text
//! cargo run -p library --features ui --example library_shot -- OUT.png [W H] [PAGE]
//! ```
//!
//! `W H` in points (default 1194 834, an 11" iPad sideways; 834 1194
//! upright). `PAGE`: `home` (default), `setlists`, `songs`, `albums`,
//! `sessions`, `rigs`, `setlist` (tonight's, with its fourth entry
//! picked), `song` (the one with three arrangements), `album`, `session`.

use anyrender::ImageRenderer as _;
use anyrender_vello::VelloImageRenderer;
use blitz_dom::{Document as _, DocumentConfig};
use blitz_traits::shell::{ColorScheme, Viewport as Screen};
use dioxus::prelude::*;
use dioxus_native_dom::DioxusDocument;
use library::ui::{
    InitialRoute, LibraryApp, LibraryData, Player, PlayerSheet, Route, SessionStart, SessionTab,
    Viewport,
};

/// Pixels per point, as on an iPad.
const SCALE: f32 = 2.0;

fn route(page: &str) -> Route {
    let lib = library::fixture::library();
    match page {
        "setlists" => Route::Setlists,
        "songs" => Route::Songs,
        "sessions" => Route::Sessions,
        "rigs" => Route::Rigs,
        "set" => Route::Setlist(
            lib.setlists
                .iter()
                .find(|s| s.tonight)
                .expect("tonight's set")
                .id,
            None,
        ),
        "setlist" => {
            let set = lib
                .setlists
                .iter()
                .find(|s| s.tonight)
                .expect("tonight's set");
            Route::Setlist(set.id, set.entries.get(4).map(|e| e.id))
        }
        "song" => {
            let song = lib
                .songs
                .iter()
                .max_by_key(|s| s.arrangements.len())
                .expect("a song");
            Route::Song(song.id, song.arrangements.get(1).map(|a| a.id))
        }
        "album" => Route::Album(lib.albums[0].id),
        "projects" => Route::Projects,
        "project" => Route::Project(lib.projects[1].id),
        "live" => Route::Project(lib.projects[0].id),
        "session" | "status" | "mini-session" => Route::Session(lib.sessions[0].id),
        _ => Route::Home,
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "library.png".into());
    let w: f32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(1194.0);
    let h: f32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(834.0);
    let page = args.next().unwrap_or_else(|| "home".into());

    let first = route(&page);
    let status = page == "status";
    // The player's state for a page of it: 0 none, 1 the mini player,
    // 2 Now Playing, 3 its queue.
    let sheet = match page.as_str() {
        "mini" | "mini-session" => 1,
        "playing" => 2,
        "queue" => 3,
        _ => 0,
    };
    let vdom = VirtualDom::new_with_props(
        Shot,
        ShotProps {
            size: (w, h),
            first,
            status,
            sheet,
        },
    );
    let (pw, ph) = ((w * SCALE) as u32, (h * SCALE) as u32);
    let mut document = DioxusDocument::new(
        vdom,
        DocumentConfig {
            viewport: Some(Screen::new(pw, ph, SCALE, ColorScheme::Dark)),
            ..Default::default()
        },
    );
    document.initial_build();
    for _ in 0..12 {
        document.poll(None);
        document.inner_mut().resolve(0.0);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let mut image = VelloImageRenderer::new(pw, ph);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            let mut inner = document.inner_mut();
            blitz_paint::paint_scene(painter, &mut inner, f64::from(SCALE), pw, ph, 0, 0);
        },
        &mut buffer,
    );
    let image =
        image::RgbaImage::from_raw(pw, ph, buffer).expect("a buffer the size of the picture");
    image.save(&out).expect("write the picture");
}

#[component]
fn Shot(size: (f32, f32), first: Route, status: bool, sheet: u8) -> Element {
    let lib = use_context_provider(|| LibraryData(Signal::new(library::fixture::library())));
    if sheet > 0 {
        use_context_provider(|| {
            let mut p = Player::new();
            p.queue.set(lib.0.peek().new_mixes());
            p.at.set(71.0);
            p.playing.set(true);
            p.from.set("New mixes".to_owned());
            p.sheet.set(match sheet {
                2 => PlayerSheet::Playing,
                3 => PlayerSheet::Queue,
                _ => PlayerSheet::Mini,
            });
            p
        });
    }
    use_context_provider(|| Viewport(Signal::new(size)));
    use_context_provider(|| InitialRoute(first));
    if status {
        use_context_provider(|| SessionStart(SessionTab::Status));
    }
    rsx! {
        // A real `<style>` node: a headless document has no head for
        // `document::Style` to land in.
        style { {"html,body{margin:0;padding:0;height:100%;background:#0c0c0f;overflow:hidden;}*{box-sizing:border-box;}"} }
        style { {library::ui::SHEET_CSS} }
        LibraryApp {}
    }
}
