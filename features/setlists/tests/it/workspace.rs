//! A song's workspace (`setlists::workspace`): where its panes go, and
//! that the progress, the chart and the lyrics follow one clock — the
//! bar, the part and the line at the playhead, and a click on any of them
//! seeking it — on a chart engraved as the song view engraves it.
#![cfg(feature = "workspace")]
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::panic
)]

use setlists::workspace::{
    Arrangement, Clock, Form, LyricsSource, NARROW_BELOW, Pane, Song, chart_fence, lyrics_source,
};

/// "Joy to the World": an intro, two verses and two choruses with a timed
/// lyric lane each, an outro; 100 bpm in 2/4, bar 1 a second into the
/// recording. Long enough for two pages.
const JOY: &str = include_str!("../fixtures/joy.kf");

/// Silent Night with an LRC of its own.
const SILENT_NIGHT: &str = "Silent Night - Franz Gruber\n72bpm 3/4 #A\n\\downbeat = 0.5\n\nVS 12\nA A A A\nE E A A\nD D A A\n";
const SILENT_NIGHT_LRC: &str =
    "[ti:Silent Night]\n[00:00.50]Silent night, holy night\n[00:05.50]All is calm, all is bright\n";

/// A bar of 2/4 at 100 bpm.
const BAR: f64 = 1.2;

fn joy() -> Song {
    Song::engrave(JOY).unwrap()
}

#[test]
fn wide_puts_the_editor_over_the_lyrics_beside_the_chart() {
    assert_eq!(Form::of(1440.0), Form::Wide);
    assert_eq!(Form::of(NARROW_BELOW), Form::Wide);
    // Not measured yet (a view drawn remotely never is): wide.
    assert_eq!(Form::of(0.0), Form::Wide);
    assert_eq!(
        Arrangement::of(Form::Wide, true),
        Arrangement::Columns {
            left: vec![Pane::Editor, Pane::Lyrics],
            right: vec![Pane::Chart],
        }
    );
    // The editor hidden: the lyrics take the left column.
    assert_eq!(
        Arrangement::of(Form::Wide, false),
        Arrangement::Columns {
            left: vec![Pane::Lyrics],
            right: vec![Pane::Chart],
        }
    );
}

#[test]
fn a_phone_stacks_progress_chart_and_lyrics_the_editor_behind_a_toggle() {
    assert_eq!(Form::of(390.0), Form::Narrow);
    assert_eq!(
        Arrangement::of(Form::Narrow, false),
        Arrangement::Stack(vec![Pane::Progress, Pane::Chart, Pane::Lyrics])
    );
    assert_eq!(
        Arrangement::of(Form::Narrow, true),
        Arrangement::Stack(vec![
            Pane::Progress,
            Pane::Editor,
            Pane::Chart,
            Pane::Lyrics
        ])
    );
}

#[test]
fn the_clock_plays_pauses_seeks_and_stops_at_the_end() {
    let mut clock = Clock::new(Some(10.0));
    assert_eq!(clock.now(5.0), 0.0);
    assert!(!clock.playing(5.0));
    clock.play(100.0);
    assert!(clock.playing(101.0));
    assert!((clock.now(102.5) - 2.5).abs() < 1e-9);
    // A seek while playing plays on from there.
    clock.seek(6.0, 103.0);
    assert!(clock.playing(103.0));
    assert!((clock.now(104.0) - 7.0).abs() < 1e-9);
    // It stops at the end.
    assert_eq!(clock.now(200.0), 10.0);
    assert!(!clock.playing(200.0));
    // Played again at the end, from the top.
    clock.play(200.0);
    assert!((clock.now(201.0) - 1.0).abs() < 1e-9);
    clock.pause(202.0);
    assert!((clock.now(300.0) - 2.0).abs() < 1e-9);
    // A seek while paused stays paused.
    clock.seek(4.0, 300.0);
    assert!(!clock.playing(301.0));
    assert_eq!(clock.now(400.0), 4.0);
}

#[test]
fn the_parts_are_the_charts_sections_on_its_own_clock() {
    let song = joy();
    let names: Vec<&str> = song.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names.len(), 6, "{names:?}");
    assert!(names[1].starts_with("Verse"), "{names:?}");
    assert!(names[2].starts_with("Chorus"), "{names:?}");
    // Bar 1 at the downbeat, a second into the recording; the intro's
    // four bars before the verse.
    assert!((song.parts[0].start - 1.0).abs() < 1e-6);
    assert!((song.parts[1].start - (1.0 + 4.0 * BAR)).abs() < 1e-6);
    // The parts tile the song.
    for pair in song.parts.windows(2) {
        assert!((pair[0].end - pair[1].start).abs() < 1e-6);
    }
    let (start, end) = song.span();
    assert!((start - 1.0).abs() < 1e-6);
    assert!((end - (1.0 + 40.0 * BAR)).abs() < 1e-6);
    assert!(
        song.chart.pages.len() >= 2,
        "{} pages",
        song.chart.pages.len()
    );
    assert!(song.chart.pages[0].starts_with("<svg"));
}

#[test]
fn the_bar_part_page_and_line_at_the_playhead_follow_one_clock() {
    let song = joy();
    let lyrics = lyrics_source(None, Some(JOY)).unwrap();
    assert!(matches!(lyrics, LyricsSource::ChartLane(_)));
    let (_, end) = song.span();
    let lines = lyrics.lyrics(end);

    // Before the downbeat: no bar, the first part and page.
    assert_eq!(song.bar_at(0.5), None);
    assert_eq!(song.part_at(0.5), Some(0));
    assert_eq!(song.page_at(0.5), 0);
    assert_eq!(song.progress(0.5), 0.0);

    // Bar 6 (the verse's second) is where the clock says.
    let t = 1.0 + 5.5 * BAR;
    let bar = song.bar(song.bar_at(t).unwrap()).unwrap();
    assert_eq!(bar.number, 6);
    assert_eq!(song.part_at(t), Some(1));
    // The verse's first line is sung from its first bar.
    let first = &lines.lines[0];
    assert!(first.text.starts_with("Joy to"), "{:?}", first.text);
    assert!(
        (first.start - (1.0 + 4.0 * BAR)).abs() < 1e-2,
        "{}",
        first.start
    );
    assert_eq!(lines.line_at(t), Some(0));

    // The last bar is on the last page; past the end, the page stays.
    let last = song.chart.bars.last().unwrap();
    assert_eq!(song.page_at(last.start + 0.1), last.page);
    assert_eq!(song.page_at(end + 30.0), last.page);
    assert!(last.page >= 1);
    assert_eq!(song.progress(end), 100.0);
}

#[test]
fn a_click_on_a_bar_a_part_or_a_line_seeks_there() {
    let song = joy();
    let (_, end) = song.span();
    let mut clock = Clock::new(Some(end));
    // A bar: its start.
    let bar = song.chart.bars[20];
    clock.seek(bar.start, 0.0);
    assert_eq!(song.bar_at(clock.now(0.0)), Some(20));
    // A part (the progress bar's section click): its start, and the bar
    // there is the part's first.
    let chorus = song.parts[2].clone();
    clock.seek(chorus.start, 0.0);
    assert_eq!(song.part_at(clock.now(0.0)), Some(2));
    assert_eq!(
        song.bar(song.bar_at(clock.now(0.0)).unwrap())
            .unwrap()
            .start,
        chorus.start
    );
    // A lyric line: its start, and the line lit there is that line.
    let lines = lyrics_source(None, Some(JOY)).unwrap().lyrics(end);
    let line = &lines.lines[3];
    clock.seek(line.start, 0.0);
    assert_eq!(lines.line_at(clock.now(0.0)), Some(3));
    // Back a section from just into the chorus: the verse before; from
    // well into it, its own start. On: the next.
    assert!((song.back_from(chorus.start + 0.5) - song.parts[1].start).abs() < 1e-9);
    assert!((song.back_from(chorus.start + 3.0) - chorus.start).abs() < 1e-9);
    assert_eq!(song.forward_from(chorus.start), Some(song.parts[3].start));
}

#[test]
fn an_edit_is_engraved_again() {
    let before = joy();
    // A verse cut to half: four bars fewer, every later part earlier.
    let edited = JOY.replacen(
        "VS 8\nD D A D\nD G A D\n[lyrics lrc]\n@1 Joy @1:2 to @2 the @2:2 world @3 the @3:2 Lord @4 is @4:2 come\n@5 Let @5:2 earth @6 re- @6:2 ceive @7 her @8 King\n",
        "VS 4\nD D A D\n",
        1,
    );
    assert_ne!(edited, JOY);
    let after = Song::engrave(&edited).unwrap();
    assert!((before.span().1 - after.span().1 - 4.0 * BAR).abs() < 1e-6);
    assert!((after.parts[2].start - (before.parts[2].start - 4.0 * BAR)).abs() < 1e-6);
}

#[test]
fn one_lyrics_source_the_lrc_before_the_charts_lane() {
    // An LRC wins over a chart's lane.
    let lrc = lyrics_source(Some(SILENT_NIGHT_LRC), Some(JOY)).unwrap();
    assert_eq!(lrc, LyricsSource::Lrc(SILENT_NIGHT_LRC.to_owned()));
    let lines = lrc.lyrics(20.0);
    assert_eq!(lines.lines.len(), 2);
    assert!((lines.lines[1].start - 5.5).abs() < 1e-9);
    // An LRC with no timed line is no source: the lane is.
    assert!(matches!(
        lyrics_source(Some("[ti:Joy]\n"), Some(JOY)),
        Some(LyricsSource::ChartLane(_))
    ));
    // Neither: none.
    assert_eq!(lyrics_source(None, Some(SILENT_NIGHT)), None);
}

#[test]
fn the_chart_is_read_from_its_page_when_its_file_cannot_be() {
    let body = format!("# Joy\n\n````kf media=Media/Joy.ogg\n{JOY}````\n\n![[Joy.lrc]]\n");
    assert_eq!(chart_fence(&body).as_deref(), Some(JOY));
    assert_eq!(chart_fence("```lrc\n[00:01]x\n```\n"), None);
}

#[test]
fn each_line_is_in_the_part_it_is_sung_in() {
    let song = joy();
    let lines = song.lyrics(&lyrics_source(None, Some(JOY)).unwrap());
    let spans: Vec<_> = song.spans().into_iter().map(|(s, _)| s).collect();
    let sections = lines.sections(&spans);
    for (k, line) in lines.lines.iter().enumerate() {
        let part = song.part_at(line.start + 1e-3).unwrap();
        let placed = sections.iter().position(|s| s.lines.contains(&k));
        assert_eq!(
            placed,
            Some(part),
            "line {k} {:?} at {} ({}–{}) is sung in {} but placed in {:?}",
            line.text,
            line.start,
            line.start,
            line.end,
            song.parts[part].name,
            placed.map(|p| &song.parts[p].name),
        );
    }
}
