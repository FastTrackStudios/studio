//! A setlist's share page, built as Architect builds a share link's site:
//! two songs brought in with their charts, an LRC for one and a timed
//! lyric lane in the other's chart, a second of generated audio each; the
//! setlist in two named sets; the site drawn with Studio's blocks.
//!
//! What the page player (`architect-sites`' `assets/player.js`) reads is
//! asserted on the HTML: a `[data-follow-playhead]` box whose `[data-at]`
//! items (seconds) ascend in document order, tied by `data-media` to the
//! URL the link's queue plays (`<link>/<vault path, %-encoded>`).
#![cfg(feature = "share")]
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::panic,
    clippy::too_many_lines
)]

use std::path::Path;

use keyflow::text::bar_times::{BarClock, bar_times};
use setlists::chart::{SongFiles, import_song};
use setlists::follow::follow_chart;
use setlists::layout::{Library, SongRef, setlist_note};
use setlists::setlist::Setlist;
use setlists::vault::Vault;

/// Franz Gruber's tune (1818), long enough for two pages.
fn silent_night() -> String {
    let verse = "VS 12\nA A A A\nE E A A\nD D A A\n\n";
    format!(
        "Silent Night - Franz Gruber\n72bpm 3/4 #A\n\\downbeat = 0.5\n\n{}",
        verse.repeat(8)
    )
}

const SILENT_NIGHT_LRC: &str =
    "[ti:Silent Night]\n[00:00.50]Silent night, holy night\n[00:05.50]All is calm, all is bright\n";

/// Isaac Watts' text (1719) on a timed lyric lane, no LRC.
const JOY: &str = "Joy to the World - Isaac Watts\n100bpm 2/4 #D\n\\downbeat = 1\n\nVS 8\nD D A D\nD G A D\n[lyrics lrc]\n@1 Joy @1:2 to @2 the @2:2 world @3 the @3:2 Lord @4 is @4:2 come\n@5 Let @5:2 earth @6 re- @6:2 ceive @7 her @8 King\n";

/// One second of a 440 Hz sine, 16-bit mono WAV.
fn sine_wav() -> Vec<u8> {
    let rate: u32 = 22_050;
    let samples: Vec<i16> = (0..rate)
        .map(|n| {
            let t = f64::from(n) / f64::from(rate);
            let v = (t * 440.0 * std::f64::consts::TAU).sin() * 8000.0;
            #[allow(clippy::cast_possible_truncation)]
            let v = v as i16;
            v
        })
        .collect();
    let data = u32::try_from(samples.len() * 2).unwrap();
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}

/// Every `name="…"` value in `html`, in document order.
fn values<'a>(html: &'a str, name: &str) -> Vec<&'a str> {
    let needle = format!(" {name}=\"");
    html.match_indices(&needle)
        .map(|(i, _)| {
            let rest = &html[i + needle.len()..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect()
}

/// `url` (relative) from the page at `page` (an absolute path ending in
/// `/`), as the browser reads it.
fn resolve(page: &str, url: &str) -> String {
    let mut parts: Vec<&str> = page.trim_end_matches('/').split('/').collect();
    let mut rest = url;
    while let Some(after) = rest.strip_prefix("../") {
        parts.pop();
        rest = after;
    }
    format!("{}/{rest}", parts.join("/"))
}

/// The block Studio drew in `html` (the site's box around it).
fn studio_blocks(html: &str) -> Vec<&str> {
    html.match_indices("data-by=\"studio.setlists\">")
        .map(|(i, m)| {
            let rest = &html[i + m.len()..];
            // The site closes its box with a line's end; what a block
            // draws closes its own `<div>`s on the same line.
            let end = rest.find("</div>\n").unwrap_or(rest.len());
            &rest[..end]
        })
        .collect()
}

/// A bar's time (`values(svg, BAR_AT)`), and a beat's.
const BAR_AT: &str = "class=\"kf-bar\" data-at";
const BEAT_AT: &str = "class=\"kf-beat\" data-at";

fn seconds(values: &[&str]) -> Vec<f64> {
    values.iter().map(|v| v.parse::<f64>().unwrap()).collect()
}

/// The chart's own SVG goes through Architect's sanitiser for blocks as it
/// is: every bar's time, the follow marks and the scoped sheet kept.
#[test]
fn the_follow_svg_survives_the_sanitiser() {
    let source = silent_night();
    let chart = follow_chart(&source, Some("../Media/Silent%20Night.wav")).unwrap();
    assert!(chart.pages >= 2, "a chart of {} pages", chart.pages);
    let scope = "site-block-0123456789abcdef";
    let kept = architect_sites::sanitize_block(&chart.svg, scope);
    for tag in ["<path", "<rect", "<g", "<svg"] {
        assert_eq!(
            chart.svg.matches(tag).count(),
            kept.matches(tag).count(),
            "every {tag} is kept"
        );
    }
    assert_eq!(values(&chart.svg, "data-at"), values(&kept, "data-at"));
    assert_eq!(
        values(&kept, "data-follow-playhead"),
        ["", ""],
        "bars, beats"
    );
    assert_eq!(
        values(&kept, "data-media"),
        ["../Media/Silent%20Night.wav"; 2]
    );
    for rule in [
        ".kf-bar[aria-current=time] { fill-opacity:0.180 }",
        ".kf-beat[aria-current=time] { fill-opacity:0.902 }",
    ] {
        assert!(
            kept.contains(&format!(".{scope} {rule}")),
            "{rule} is kept, under the block's scope"
        );
    }
    for tag in ["<use", "<defs"] {
        assert_eq!(
            chart.svg.matches(tag).count(),
            kept.matches(tag).count(),
            "every {tag} is kept"
        );
    }
    assert!(!kept.contains("<text"), "the text is drawn as outlines");
    assert!(
        !kept.contains("font-family"),
        "no font is needed to read it"
    );
}

/// What the page player follows: one box, its bars' times ascending in
/// document order, every bar played (a repeat's pass too), on the chart's
/// clock (`\downbeat`, its tempo and meter), across every page.
#[test]
fn the_bars_carry_the_charts_own_clock() {
    let source = silent_night();
    let chart = follow_chart(&source, None).unwrap();
    let parsed = keyflow::parse(&source).unwrap();
    let want: Vec<String> = bar_times(&parsed, BarClock::at(0.5))
        .iter()
        .map(|b| format!("{:.3}", b.start))
        .collect();
    assert_eq!(values(&chart.svg, BAR_AT), want);
    assert_eq!(want.len(), 96, "8 verses of 12 bars");
    assert_eq!(want[0], "0.500");
    // 72 bpm in 3/4: a bar is 2.5 s.
    assert_eq!(want[1], "3.000");
    let times = seconds(&values(&chart.svg, BAR_AT));
    assert!(times.windows(2).all(|w| w[0] < w[1]), "ascending");
    let beats = seconds(&values(&chart.svg, BEAT_AT));
    assert_eq!(beats.len(), 96 * 3, "a beat each of 3/4");
    assert!(beats.windows(2).all(|w| w[0] < w[1]), "ascending");
    // 72 bpm: a beat is 5/6 s.
    assert!((beats[1] - beats[0] - 60.0 / 72.0).abs() < 0.001);
    assert_eq!(chart.svg.matches("data-follow-playhead").count(), 2);
    assert_eq!(chart.svg.matches("class=\"kf-page\"").count(), chart.pages);
    assert_eq!(values(&chart.svg, BAR_AT).len(), chart.bars.len());
}

#[test]
fn a_shared_setlist_plays_with_its_charts_and_lyrics() {
    let dir = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    let put = |name: &str, bytes: &[u8]| {
        let path = files.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    };
    let vault = Vault::at(dir.path());
    let lib = Library::named("Christmas");

    let kf = put("silent.kf", silent_night().as_bytes());
    let lrc = put("silent.lrc", SILENT_NIGHT_LRC.as_bytes());
    let wav = put("Silent Night.wav", &sine_wav());
    let silent = import_song(
        &vault,
        &lib,
        &SongFiles {
            chart: Some(&kf),
            lrc: Some(&lrc),
            media: &[&wav],
            ..SongFiles::default()
        },
        None,
    )
    .unwrap();
    let kf = put("joy.kf", JOY.as_bytes());
    let wav = put("Joy to the World.wav", &sine_wav());
    let joy = import_song(
        &vault,
        &lib,
        &SongFiles {
            chart: Some(&kf),
            media: &[&wav],
            ..SongFiles::default()
        },
        None,
    )
    .unwrap();

    let at = |s: &str| SongRef::new("Christmas", s, None);
    let mut set = Setlist::new("Tree Lighting", Vec::new());
    set.date = Some("2026-12-04".to_owned());
    set.add(at("Joy to the World"), None, Some("Presidents Songs"))
        .unwrap();
    set.add(at("Silent Night"), None, Some("Tree Lighting Concert Set"))
        .unwrap();
    let known = [(at("Joy to the World"), joy), (at("Silent Night"), silent)];
    vault
        .write(&setlist_note(&set.title), set.note(&known).as_bytes())
        .unwrap();

    // The share: the setlist's note and its songs', as STUDIO_SETLISTS_SHARE
    // asks for them, drawn with Studio's blocks.
    let notes = vec![
        setlist_note(&set.title),
        lib.song_note("Joy to the World"),
        lib.song_note("Silent Night"),
    ];
    let out = tempfile::tempdir().unwrap();
    let mut opts = architect_sites::Options::new(dir.path(), "Tree Lighting");
    opts.scope = architect_sites::Scope::Notes(notes);
    let report = architect_sites::build(&opts, &setlists::share::blocks(), out.path()).unwrap();
    let read = |page: &str| {
        let path = out.path().join(page.trim_matches('/')).join("index.html");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e} ({report:?})", path.display()))
    };
    // Where the node mounts the link: what the queue's URLs start with.
    let link = "/s/hzhbdfiz7gaxwv78";
    let queue_src = |file: &str| format!("{link}/{}", architect_sites::encode_path(file));

    // ── A song's page: its chart follows its recording ──────────────
    let page = "/libraries/christmas/silent-night/silent-night/";
    let html = read(page);
    let recording = lib.song_file("Silent Night", "Media/Silent Night.wav");
    assert!(
        html.contains("data-site-media=\"audio\""),
        "its recording is a player"
    );
    let blocks = studio_blocks(&html);
    assert_eq!(blocks.len(), 1, "the chart, drawn by Studio");
    let chart = blocks[0];
    assert!(chart.contains("<svg"), "drawn, not left as text");
    assert!(
        chart.contains("data-now-playing"),
        "the song's now-playing view"
    );
    let media = values(chart, "data-media");
    assert_eq!(media.len(), 3, "the view, the bars and the beats");
    assert!(media.iter().all(|m| *m == media[0]));
    assert_eq!(
        resolve(&format!("{link}{page}"), media[0]),
        queue_src(&recording),
        "the chart's recording is the one the link's queue plays"
    );
    let parsed = keyflow::parse(silent_night()).unwrap();
    let want: Vec<String> = bar_times(&parsed, BarClock::at(0.5))
        .iter()
        .map(|b| format!("{:.3}", b.start))
        .collect();
    assert_eq!(values(chart, BAR_AT), want, "every bar timed, every page");
    assert!(
        chart.contains(".kf-bar[aria-current=time]"),
        "the sheet kept"
    );
    assert!(
        !html.contains("chart-1.png\" alt"),
        "the PNG pages are not what the page shows"
    );
    assert!(
        html.contains("Silent%20Night%20-%20Master%20Rhythm.pdf"),
        "the PDF to download"
    );
    // Its lyrics: its LRC, each line timed.
    let lyrics = &html[html.find("class=\"site-lyrics\"").unwrap()..];
    assert_eq!(values(lyrics, "data-at")[..2], ["0.500", "5.500"]);
    assert!(lyrics.contains("Silent night, holy night"));

    // ── A song with no LRC: its chart's lyric lane, timed ───────────
    let page = "/libraries/christmas/joy-to-the-world/joy-to-the-world/";
    let html = read(page);
    let lyrics = &html[html.find("class=\"site-lyrics\"").unwrap()..];
    // 100 bpm in 2/4: a bar is 1.2 s, bar 1 at 1 s.
    assert_eq!(values(lyrics, "data-at")[..2], ["1.000", "5.800"]);
    assert!(
        lyrics.contains("Joy to the world the Lord is come"),
        "{lyrics}"
    );
    assert!(lyrics.contains("Let earth receive her King"));

    // ── The setlist's page: one playlist, its sets labels in it ─────
    let page = "/setlists/tree-lighting/";
    let html = read(page);
    assert_eq!(html.matches("data-playlist=\"\"").count(), 1, "one list");
    let presidents = html
        .find("<li data-playlist-section=\"\">Presidents Songs</li>")
        .unwrap();
    let tree = html
        .find("<li data-playlist-section=\"\">Tree Lighting Concert Set</li>")
        .unwrap();
    let joy_row = html.find("data-title=\"Joy to the World\"").unwrap();
    let silent_row = html.find("data-title=\"Silent Night\"").unwrap();
    assert!(
        presidents < joy_row && joy_row < tree && tree < silent_row,
        "labels before their songs"
    );
    assert!(!html.contains("<h2"), "a set is no heading of the page");
    let list = studio_blocks(&html)[0];
    assert!(!list.contains("data-playlist-play"), "no play buttons");
    // Its one button a row: its menu.
    assert_eq!(
        list.matches("<button").count(),
        list.matches("<button type=\"button\" data-track-more=\"\" aria-label=\"More: ")
            .count(),
        "{list}"
    );
    assert_eq!(list.matches("data-track-more=\"\"").count(), 2);
    // Its own list reset, inline, and its sets joined as one playlist.
    assert!(
        list.contains(
            "data-playlist-join=\"\"><ol style=\"list-style:none; margin:0; padding:0\">"
        ),
        "{list}"
    );
    // Numbers run on through the sets; a label is no track.
    assert_eq!(values(&html, "data-track").len(), 2);
    let marks: Vec<&str> = html
        .match_indices("<span data-track-mark=\"\">")
        .map(|(i, m)| &html[i + m.len()..=i + m.len()])
        .collect();
    assert_eq!(marks, ["1", "2"]);
    // Each song's tile: its key (its chart's) on its title's colour.
    for (title, key, said) in [
        ("Joy to the World", "D", "Key of D"),
        ("Silent Night", "A", "Key of A"),
    ] {
        let hue = setlists::share::tile_hue(title);
        assert!(
            html.contains(&format!(
                "data-hue=\"{hue}\" data-key=\"\" role=\"img\" aria-label=\"{said}\" style=\"background:hsl({hue} 42% 32%)\"><span data-track-key=\"\" aria-hidden=\"true\">{key}</span></span>"
            )),
            "{title}: {html}"
        );
    }
    assert!(!html.contains("data-track-transposed"), "played as charted");
    assert!(html.contains("<span data-track-meta=\"\">"));
    // The link's files page and archive, once, after the last set: the
    // node's routes of the link, which serve only its audience's files.
    assert_eq!(
        values(&html, "href")
            .iter()
            .filter(|h| **h == "../../_archive.zip")
            .count(),
        1
    );
    assert_eq!(
        values(&html, "href")
            .iter()
            .filter(|h| **h == "../../_files/")
            .count(),
        1
    );
    let rows: Vec<String> = studio_blocks(&html)
        .iter()
        .flat_map(|b| values(b, "data-media"))
        .map(|m| resolve(&format!("{link}{page}"), m))
        .collect();
    let joy_src = queue_src(&lib.song_file("Joy to the World", "Media/Joy to the World.wav"));
    let silent_src = queue_src(&recording);
    assert_eq!(rows, [joy_src, silent_src], "its songs, in running order");
    // Each row's title and the page where it plays.
    assert_eq!(
        values(&html, "data-title"),
        ["Joy to the World", "Silent Night"]
    );
    let pages: Vec<String> = values(&html, "data-page")
        .into_iter()
        .map(|h| resolve(page, h))
        .collect();
    assert_eq!(
        pages,
        [
            "/libraries/christmas/joy-to-the-world/joy-to-the-world/",
            "/libraries/christmas/silent-night/silent-night/"
        ]
    );
    // No length, no link of its own: its page is its menu's.
    assert!(!html.contains("data-track-length") && !html.contains(">0:01</span>"));
    assert!(!html.contains("data-track-open"));
    for song in &pages {
        assert!(Path::new(&out.path().join(song.trim_matches('/'))).is_dir());
    }
}

/// A setlist page written before its sets were lines of one fence (a
/// fence a set, under `## headings`): each fence still draws as a list of
/// rows with no play buttons, numbered as written (Architect's player joins
/// them into one), and a song with no recording is a row that is no track.
#[test]
fn an_older_setlist_page_still_draws_and_a_song_with_no_recording_is_a_row() {
    let page = "Setlists/Eve.md";
    let draw = |source: &str, info: &str| {
        let claim = architect_sites::Claim::Fence("studio-setlist".to_owned());
        let block = architect_sites::Block {
            claim: &claim,
            info,
            source,
            page,
        };
        let (_, html) = setlists::share::blocks().draw(&block).unwrap();
        html.unwrap()
    };
    let first = draw(
        "1 | Sleigh Ride | Libraries/C/Sleigh Ride/Sleigh Ride.md | B · 86 bpm · 4/4 | Libraries/C/Sleigh Ride/Media/Sleigh Ride.mp4 | Tori Kelly | 186.0\n",
        "studio-setlist",
    );
    let second = draw(
        "4 | Feliz Navidad | Libraries/C/Feliz Navidad/Feliz Navidad.md\n",
        "studio-setlist downloads",
    );
    assert!(!first.contains("data-playlist-play") && !second.contains("data-playlist-play"));
    assert!(
        first.contains(
            "<button type=\"button\" data-track-more=\"\" aria-label=\"More: Sleigh Ride\""
        )
    );
    assert!(second.contains(
        "<button type=\"button\" data-track-more=\"\" aria-label=\"More: Feliz Navidad\""
    ));
    assert!(first.contains("<span data-track-mark=\"\">1</span>"));
    assert!(
        second.contains("<span data-track-mark=\"\">4</span>"),
        "numbered as written"
    );
    assert!(
        first.contains("<span data-track-meta=\"\">Tori Kelly</span>"),
        "who it is by, only"
    );
    assert!(first.contains("data-subtitle=\"Tori Kelly\""));
    // Its key, from its summary (a page written before keys had a column).
    assert!(first.contains("aria-label=\"Key of B\""));
    assert!(first.contains("<span data-track-key=\"\" aria-hidden=\"true\">B</span>"));
    assert!(!first.contains("data-track-length") && !first.contains("3:06"));
    // No recording: no track, its page still a tap away, said so.
    assert!(!second.contains("data-track=\"\""));
    assert!(second.contains("<li data-page=\"../../libraries/c/feliz-navidad/feliz-navidad/\" data-title=\"Feliz Navidad\">"), "{second}");
    assert!(second.contains(">No recording</span>"));
    assert!(!second.contains("data-track-open"));
    // No key (and no art): a music note on its colour, never letters; a
    // picture to a screen reader.
    assert!(
        second.contains("aria-hidden=\"true\" style=\"background:hsl("),
        "{second}"
    );
    assert!(second.contains("<path d=\"M9 18V5l12-2v13\">"), "{second}");
    assert!(!second.contains(">FN<"), "no initials");
}

/// A row's key: its column (`E`, `E*`, `E→F`), spoken in words; played in
/// the setlist's own key (`play`), every key in it moved and a mark saying
/// from what. The chart itself is not moved.
#[test]
fn a_rows_key_is_on_its_tile_and_a_setlists_key_moves_it() {
    let draw = |source: &str| {
        let claim = architect_sites::Claim::Fence("studio-setlist".to_owned());
        let block = architect_sites::Block {
            claim: &claim,
            info: "studio-setlist",
            source,
            page: "Setlists/Eve.md",
        };
        let (_, html) = setlists::share::blocks().draw(&block).unwrap();
        html.unwrap()
    };
    let row = |key: &str, play: &str| {
        draw(&format!(
            "1 | Song | Libraries/C/Song/Song.md | D · 90 bpm · 4/4 | Libraries/C/Song/Media/Song.mp3 | Band | 180 | {key} | {play}\n"
        ))
    };
    // Plain, with notes outside it, and starting in one key and ending in
    // another.
    let plain = row("E", "");
    assert!(plain.contains("aria-label=\"Key of E\""), "{plain}");
    assert!(plain.contains(">E</span>") && !plain.contains("data-track-transposed"));
    let outside = row("E*", "");
    assert!(outside.contains("aria-label=\"Key of E, with notes outside the key\""));
    assert!(outside.contains(">E*</span>"));
    let moves = row("D→E", "");
    assert!(moves.contains("aria-label=\"Starts in D, ends in E\""));
    assert!(!moves.contains("data-key-size"), "three letters: full size");
    // In the setlist's key: both ends moved, spelled as that key is.
    let played = row("D→E", "Eb");
    assert!(played.contains(">Eb→F</span>"), "{played}");
    assert!(played.contains("data-key-size=\"s\""));
    assert!(
        played.contains(
            "aria-label=\"Starts in E flat, ends in F, transposed up 1 semitone from D\""
        ),
        "{played}"
    );
    // How far, in the corner: one number for the whole song.
    assert!(played.contains("<span data-track-transposed=\"\" aria-hidden=\"true\">+1</span>"));
    let minor = row("C→Em", "Eb");
    assert!(minor.contains(">Eb→Gm</span>"), "{minor}");
    assert!(minor.contains(">+3</span>"));
    // Down, with a minus sign; the key alone is the tile's text.
    let down = row("G", "E");
    assert!(
        down.contains("<span data-track-transposed=\"\" aria-hidden=\"true\">\u{2212}3</span>"),
        "{down}"
    );
    assert!(
        down.contains("aria-label=\"Key of E, transposed down 3 semitones from G\""),
        "{down}"
    );
    // A tritone is +6; the same pitch spelled otherwise is no transposition.
    assert!(row("C", "F#").contains(">+6</span>"));
    let enharmonic = row("F#", "Gb");
    assert!(!enharmonic.contains("data-track-transposed") && enharmonic.contains(">F#</span>"));
    // Another mode is no transposition: the played key, no number, and
    // the chart's key said.
    let mode = row("E", "Em");
    assert!(
        !mode.contains("data-track-transposed") && mode.contains(">Em</span>"),
        "{mode}"
    );
    assert!(
        mode.contains("aria-label=\"Key of E minor; its chart is in E\""),
        "{mode}"
    );
    // The longest a label gets, smallest type.
    let longest = row("Eb→F#m", "");
    assert!(longest.contains(">Eb→F#m</span>") && longest.contains("data-key-size=\"xs\""));
    assert!(
        longest.contains("aria-label=\"Starts in E flat, ends in F sharp minor\""),
        "{longest}"
    );
    // The setlist's key the song's own: not moved.
    let same = row("E", "E");
    assert!(!same.contains("data-track-transposed") && same.contains(">E</span>"));
    // Its key column over its summary's.
    assert!(!plain.contains(">D</span>"));
}

/// White on a tile reads at 4.5:1 or more on every hue (WCAG's contrast of
/// relative luminances), the key's 10 px included.
#[test]
#[allow(
    clippy::many_single_char_names,
    clippy::suboptimal_flops,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // HSL to RGB as the formula writes it.
fn white_on_every_tile_reads_at_four_and_a_half_to_one() {
    fn channel(c: f64) -> f64 {
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    for hue in 0..360_u32 {
        let colour = setlists::share::tile_colour(hue);
        let inner = colour.trim_start_matches("hsl(").trim_end_matches(')');
        let parts: Vec<f64> = inner
            .split_whitespace()
            .map(|p| p.trim_end_matches('%').parse::<f64>().unwrap())
            .collect();
        let (h, sat, l) = (parts[0], parts[1] / 100.0, parts[2] / 100.0);
        let c = (1.0 - (2.0 * l - 1.0).abs()) * sat;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let (r, g, b) = match (h / 60.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c / 2.0;
        let lum = 0.2126 * channel(r + m) + 0.7152 * channel(g + m) + 0.0722 * channel(b + m);
        let contrast = 1.05 / (lum + 0.05);
        assert!(contrast >= 4.5, "hue {hue}: {colour} is {contrast:.2}:1");
    }
}
