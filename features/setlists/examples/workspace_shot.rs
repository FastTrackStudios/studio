//! A song's workspace as a picture: the song view's tree
//! (`setlists::workspace_view::SongWorkspace`) in a headless Blitz
//! document, painted on the CPU to a PNG — the way Session's
//! `blitz_shot` checks its panels.
//!
//! ```sh
//! cargo run -p setlists --features extension --example workspace_shot -- \
//!     /tmp/workspace.png [wide|narrow] [seconds] [WxH] [chart.kf] [song.lrc]
//! ```
//!
//! The chart is `tests/fixtures/joy.kf` unless one is given; its lyrics
//! are the LRC given, else its chart's timed lyric lane. Nothing is
//! saved: the song is engraved as the song view engraves it and shown at
//! `seconds` (12.5 unless given).
#![allow(clippy::unwrap_used, clippy::print_stderr, clippy::print_stdout)]

use std::sync::Arc;

use anyrender::ImageRenderer as _;
use blitz_dom::{Document as _, DocumentConfig};
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;
use dioxus_native_dom::DioxusDocument;
use setlists::workspace::{ChartAccess, Form, Loaded, lyrics_source};
use setlists::workspace_view::SongWorkspace;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args
        .first()
        .cloned()
        .unwrap_or_else(|| "/tmp/workspace.png".to_owned());
    let form = match args.get(1).map(String::as_str) {
        Some("narrow") => Form::Narrow,
        _ => Form::Wide,
    };
    let at: f64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(12.5);
    let (width, height) = args
        .get(3)
        .and_then(|s| s.split_once('x'))
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .unwrap_or(if form == Form::Narrow {
            (390, 1400)
        } else {
            (1600, 1000)
        });
    // `none`: a song with no chart.
    let source = match args.get(4).map(String::as_str) {
        None => include_str!("../tests/fixtures/joy.kf").to_owned(),
        Some("none") => String::new(),
        Some(p) => std::fs::read_to_string(p).unwrap(),
    };
    let lrc = args.get(5).map(|p| std::fs::read_to_string(p).unwrap());
    // `FTS_WS_READONLY=1`: the note's copy of a `.kf` that could not be read.
    let chart = if std::env::var_os("FTS_WS_READONLY").is_some() {
        ChartAccess::open(
            Some("Libraries/Christmas/Joy/Joy.kf".to_owned()),
            Some(Err("permission denied".to_owned())),
            Some(source.clone()),
        )
    } else {
        ChartAccess::open(None, None, Some(source.clone()))
    };
    let loaded = Loaded {
        title: "Joy to the World".to_owned(),
        artist: Some("Isaac Watts".to_owned()),
        lyrics: lyrics_source(lrc.as_deref(), Some(&source)),
        chart,
        ..Loaded::default()
    };

    let vdom = VirtualDom::new_with_props(Still, StillProps { loaded, form, at });
    let mut document = DioxusDocument::new(
        vdom,
        DocumentConfig {
            viewport: Some(Viewport::new(width, height, 1.0, ColorScheme::Dark)),
            net_provider: Some(Arc::new(DataUris)),
            ..Default::default()
        },
    );
    document.initial_build();
    // Engraved on its own thread a pause after it opens, measured, and its
    // page fetched as an image: rounds until the page is up.
    for _ in 0..120 {
        document.poll(None);
        document.inner_mut().resolve(0.0);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let mut image = anyrender_vello_cpu::VelloCpuImageRenderer::new(width, height);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            let mut inner = document.inner_mut();
            blitz_paint::paint_scene(painter, &mut inner, 1.0, width, height, 0, 0);
        },
        &mut buffer,
    );
    image::RgbaImage::from_raw(width, height, buffer)
        .unwrap()
        .save(&out)
        .unwrap();
    println!("{out}");
}

#[derive(Props, Clone, PartialEq)]
struct StillProps {
    loaded: Loaded,
    form: Form,
    at: f64,
}

#[allow(non_snake_case)]
fn Still(props: StillProps) -> Element {
    rsx! {
        div { style: "position:absolute; top:0; left:0; width:100%; height:100%;",
            SongWorkspace { loaded: props.loaded, form: props.form, at: props.at }
        }
    }
}

/// Answers `data:` URLs (base64 or %-escaped), now; nothing else.
struct DataUris;

impl blitz_traits::net::NetProvider for DataUris {
    fn fetch(
        &self,
        _doc: usize,
        request: blitz_traits::net::Request,
        handler: Box<dyn blitz_traits::net::NetHandler>,
    ) {
        use base64::Engine as _;
        let url = request.url.as_str();
        let Some((head, body)) = url.strip_prefix("data:").and_then(|p| p.split_once(',')) else {
            return;
        };
        let bytes = if head.ends_with(";base64") {
            base64::engine::general_purpose::STANDARD
                .decode(body)
                .unwrap_or_default()
        } else {
            body.as_bytes().to_vec()
        };
        handler.bytes(url.to_owned(), bytes.into());
    }
}
