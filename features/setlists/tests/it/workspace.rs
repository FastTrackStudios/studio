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

/// The reviewer's sequence (studio#430): save an edit; the `.kf` cannot be
/// read; reopen the song; access comes back. The old copy in the note
/// must never be written over the saved edit.
#[test]
fn an_old_copy_of_the_chart_never_overwrites_the_saved_kf() {
    use setlists::workspace::{ChartAccess, ChartFrom, Saving, Write};
    let file = "Libraries/Christmas/Joy/Joy.kf".to_owned();
    let old = JOY.to_owned(); // the note's ```kf copy, written at import
    let edited = JOY.replace("Joy to the World", "Joy to the World (edited)");

    // Opened from the `.kf`, an edit saved to it.
    let opened = ChartAccess::open(Some(file.clone()), Some(Ok(old.clone())), Some(old.clone()));
    assert_eq!(opened.from, ChartFrom::File);
    let mut saving = Saving::new(opened.text);
    assert_eq!(
        saving.decide(Ok(&old), &edited, 40),
        Write::Save { backup: true }
    );
    saving.saved(edited.clone(), 40);

    // Reopened while the `.kf` cannot be read: the note's (old) copy,
    // read-only, saying why.
    let reopened = ChartAccess::open(
        Some(file.clone()),
        Some(Err("permission denied".to_owned())),
        Some(old.clone()),
    );
    assert_eq!(reopened.text, old);
    let why = reopened.editable().unwrap_err();
    assert!(
        why.contains("read-only") && why.contains("permission denied"),
        "{why}"
    );

    // Access comes back: the `.kf` is read again before anything can be
    // written, and the editor holds the saved edit, not the old copy.
    let back = reopened.reread(Ok(edited.clone()));
    assert_eq!(back.from, ChartFrom::File);
    assert_eq!(back.text, edited);
    assert!(back.editable().is_ok());
    // Read again and still unreadable: still read-only.
    let still = ChartAccess::open(
        Some(file),
        Some(Err("denied".to_owned())),
        Some(old.clone()),
    )
    .reread(Err("denied again".to_owned()));
    assert!(still.editable().is_err());
    assert_eq!(still.text, old);
}

#[test]
fn a_save_reads_the_kf_again_and_never_writes_over_a_change() {
    use setlists::workspace::{Saving, Write};
    let mut saving = Saving::new(JOY.to_owned());
    saving.knows_bars(40);
    let edit = JOY.replace("100bpm", "104bpm");
    // Changed on disk since it was read (another edit, elsewhere): refused.
    let elsewhere = JOY.replace("Isaac Watts", "I. Watts");
    assert!(matches!(
        saving.decide(Ok(&elsewhere), &edit, 40),
        Write::Refuse { why, reread: true } if why.contains("changed")
    ));
    // Unreadable now: refused.
    assert!(matches!(
        saving.decide(Err("permission denied"), &edit, 40),
        Write::Refuse { why, reread: true } if why.contains("permission denied")
    ));
    // As it was: saved, backed up the first time only.
    assert_eq!(
        saving.decide(Ok(JOY), &edit, 40),
        Write::Save { backup: true }
    );
    saving.saved(edit.clone(), 40);
    let again = edit.replace("104bpm", "108bpm");
    assert_eq!(
        saving.decide(Ok(&edit), &again, 40),
        Write::Save { backup: false }
    );
    // What it holds already: nothing to write.
    assert_eq!(saving.decide(Ok(&edit), &edit, 40), Write::Unchanged);
}

#[test]
fn a_chart_cut_to_no_bars_is_not_saved_over_bars_without_a_yes() {
    use setlists::workspace::{Saving, Write};
    let header = "Joy to the World - Isaac Watts\n100bpm 2/4 #D\n";
    // Keyflow accepts it: a chart with no bars.
    assert_eq!(Song::engrave(header).map_or(0, |s| s.chart.bars.len()), 0);
    let mut saving = Saving::new(JOY.to_owned());
    saving.knows_bars(40);
    assert!(matches!(
        saving.decide(Ok(JOY), header, 0),
        Write::Confirm(q) if q.contains("no bars") && q.contains("40")
    ));
    assert_eq!(saving.asking(), Some(header));
    // The person says yes with the editor still holding it.
    assert!(saving.confirm(header));
    assert_eq!(
        saving.decide(Ok(JOY), header, 0),
        Write::Save { backup: true }
    );
}

#[test]
fn the_backup_is_in_the_extensions_folder_not_the_vault() {
    use setlists::workspace::backup_path;
    assert_eq!(
        backup_path("Libraries/Christmas/Joy/Joy.kf", 1_794_000_000),
        "backups/Libraries/Christmas/Joy/Joy.1794000000.kf"
    );
    // Relative: the host's `write_file` puts it in the extension's own
    // folder.
    assert!(!backup_path("/Libraries/X/X.kf", 1).starts_with('/'));
}

#[test]
fn a_song_that_names_no_kf_edits_its_notes_copy_and_never_saves() {
    use setlists::workspace::{ChartAccess, ChartFrom};
    let chart = ChartAccess::open(None, None, Some(JOY.to_owned()));
    assert_eq!(chart.from, ChartFrom::NoteOnly);
    assert!(chart.editable().is_ok());
    assert_eq!(chart.file, None);
}

/// The fake `.kf` a sequence of saves writes, as the workspace's save path
/// does: read it, decide, back up the first time, write.
struct Disk {
    kf: String,
    backups: Vec<(String, String)>,
}

impl Disk {
    fn save(&mut self, saving: &mut setlists::workspace::Saving, text: &str, bars: usize) {
        use setlists::workspace::{Write, backup_path};
        saving.now_saving(text);
        if saving.base() == text {
            return;
        }
        let on_disk = self.kf.clone();
        if let Write::Save { backup } = saving.decide(Ok(&on_disk), text, bars) {
            if backup {
                let path = backup_path("Libraries/Christmas/Joy/Joy.kf", 7);
                assert!(
                    !self.backups.iter().any(|(p, _)| *p == path),
                    "the backup of the file as opened is written over"
                );
                self.backups.push((path, saving.base().to_owned()));
            }
            text.clone_into(&mut self.kf);
            saving.saved(text.to_owned(), bars);
        }
    }
}

/// The re-check's sequence (studio#430): cut the chart to its header (the
/// question comes, nothing is written), put the bars back (saved), then
/// "Save anyway", still on screen. It must not write the header.
#[test]
fn a_stale_save_anyway_never_writes_no_bars_over_a_good_chart() {
    use setlists::workspace::Saving;
    let header = "Joy to the World - Isaac Watts\n100bpm 2/4 #D\n";
    let edited = JOY.replace("100bpm", "104bpm");
    let mut disk = Disk {
        kf: JOY.to_owned(),
        backups: Vec::new(),
    };
    let mut saving = Saving::new(JOY.to_owned());
    saving.knows_bars(40);

    // Cut to its header: asked, not written.
    disk.save(&mut saving, header, 0);
    assert_eq!(disk.kf, JOY);
    assert_eq!(saving.asking(), Some(header));

    // The bars back (an edit with bars): the question goes, it is saved.
    disk.save(&mut saving, &edited, 40);
    assert_eq!(disk.kf, edited);
    assert_eq!(saving.asking(), None);

    // "Save anyway", still on screen, with the editor holding the full
    // chart: it counts for nothing, and nothing changes on disk.
    assert!(!saving.confirm(&edited));
    disk.save(&mut saving, &edited, 40);
    assert_eq!(disk.kf.as_bytes(), edited.as_bytes());
    // Nor does an old copy of the question say yes to the header later.
    assert!(!saving.confirm(header));
    disk.save(&mut saving, header, 0);
    assert_eq!(disk.kf.as_bytes(), edited.as_bytes());
}

/// The question goes too when the text comes back to what the file holds
/// (the early return before deciding).
#[test]
fn putting_the_chart_back_as_it_was_drops_the_question() {
    use setlists::workspace::{Saving, Write};
    let header = "Joy to the World - Isaac Watts\n100bpm 2/4 #D\n";
    let mut saving = Saving::new(JOY.to_owned());
    saving.knows_bars(40);
    assert!(matches!(
        saving.decide(Ok(JOY), header, 0),
        Write::Confirm(_)
    ));
    saving.now_saving(JOY);
    assert_eq!(saving.asking(), None);
    assert!(!saving.confirm(header));
}

#[test]
fn a_kf_changed_on_disk_is_read_again_and_the_unsaved_edit_kept() {
    use setlists::workspace::{Saving, Write, unsaved_path};
    let mut saving = Saving::new(JOY.to_owned());
    let mine = JOY.replace("100bpm", "104bpm");
    let theirs = JOY.replace("Isaac Watts", "I. Watts");
    // Refused, and the way on is reading it again (the button shows).
    assert!(matches!(
        saving.decide(Ok(&theirs), &mine, 40),
        Write::Refuse { reread: true, .. }
    ));
    // Read again: saving starts from theirs; mine is kept beside the
    // backups, in the extension's folder, not the vault.
    let saving = saving.reread(theirs.clone());
    assert_eq!(saving.base(), theirs);
    assert_eq!(
        unsaved_path("Libraries/Christmas/Joy/Joy.kf", 7, 9),
        "backups/Libraries/Christmas/Joy/Joy.7.unsaved-9.kf"
    );
}

#[test]
fn a_reread_writes_the_unsaved_edit_before_the_editor_takes_the_file() {
    use std::cell::RefCell;

    use setlists::workspace::{Reread, Unsaved, keep_unsaved, unsaved_path};
    let mine = JOY.replace("100bpm", "104bpm");
    let theirs = JOY.replace("Isaac Watts", "I. Watts");
    let path = || unsaved_path("Joy/Joy.kf", 7, 9);
    // Edited since the last save, and not what the file holds now: kept.
    let edit = Unsaved::of(true, &mine, &theirs, Some(JOY), path).unwrap();
    let written = RefCell::new(Vec::new());
    let done = futures::executor::block_on(keep_unsaved(Some(edit), |path, text| {
        written.borrow_mut().push((path, text));
        async { Ok(()) }
    }));
    assert_eq!(
        written.into_inner(),
        vec![("backups/Joy/Joy.7.unsaved-9.kf".to_owned(), mine.clone())],
        "the editor's text, written to the extension's folder"
    );
    assert_eq!(
        done,
        Reread::Take {
            kept: Some("backups/Joy/Joy.7.unsaved-9.kf".to_owned())
        }
    );
    // Not written: the editor holds on to the edit and says why.
    let edit = Unsaved::of(true, &mine, &theirs, Some(JOY), path);
    let held = futures::executor::block_on(keep_unsaved(edit, |_, _| async {
        Err("the disk is full".to_owned())
    }));
    assert!(matches!(&held, Reread::Hold { why } if why.contains("the disk is full")));
    // Nothing to keep: the text saving started from, the file's own, or
    // the note's read-only copy.
    assert_eq!(Unsaved::of(true, JOY, &theirs, Some(JOY), path), None);
    assert_eq!(Unsaved::of(true, &theirs, &theirs, Some(JOY), path), None);
    assert_eq!(Unsaved::of(false, &mine, &theirs, Some(JOY), path), None);
    let nothing = futures::executor::block_on(keep_unsaved(None, |_, _| async {
        Err("never called".to_owned())
    }));
    assert_eq!(nothing, Reread::Take { kept: None });
}

#[test]
fn the_beat_at_the_playhead_is_on_its_bars_page() {
    let song = joy();
    assert_ne!(song.chart.beats.len(), 0);
    // Mid-way through bar 21 (2/4: two beats), its second beat.
    let bar = song.chart.bars[20];
    let t = f64::midpoint(bar.start, bar.end) + 0.01;
    let beat = song.beat_at(t).unwrap();
    assert_eq!(beat.bar + 1, bar.number);
    assert_eq!(beat.beat, 1);
    assert_eq!(beat.page, bar.page);
    // The line inside the bar's box on its own page.
    assert!(beat.x >= bar.x - 1.0 && beat.x <= bar.x + bar.width + 1.0);
    assert!(beat.y >= 0.0 && beat.y < song.chart.height);
    assert_eq!(song.beat_at(-5.0), None);
}

#[test]
fn two_charts_in_one_window_draw_no_id_twice() {
    let ids = |song: &Song| -> Vec<String> {
        song.chart
            .pages
            .iter()
            .flat_map(|page| {
                page.split(" id=\"")
                    .skip(1)
                    .filter_map(|r| r.split('"').next().map(str::to_owned))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let joy = ids(&joy());
    let night = ids(&Song::engrave(SILENT_NIGHT).unwrap());
    assert_ne!(joy.len().min(night.len()), 0);
    assert!(joy.iter().all(|id| !night.contains(id)));
    // Nor two pages of one chart.
    let mut all = joy.clone();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), joy.len());
}

#[test]
fn a_reread_never_backs_the_file_up_again_over_the_original() {
    use setlists::workspace::{Saving, Write};
    let mut saving = Saving::new(JOY.to_owned());
    let edit = JOY.replace("100bpm", "104bpm");
    assert_eq!(
        saving.decide(Ok(JOY), &edit, 40),
        Write::Save { backup: true }
    );
    saving.saved(edit.clone(), 40);
    // Read again (after a refusal, or "Allow saving"): the next save does
    // not back up the edited file over the original's backup.
    let mut saving = saving.reread(edit.clone());
    let again = edit.replace("104bpm", "108bpm");
    assert_eq!(
        saving.decide(Ok(&edit), &again, 40),
        Write::Save { backup: false }
    );
    // A session that never saved before its re-read still backs up once.
    let mut fresh = Saving::new(JOY.to_owned()).reread(JOY.to_owned());
    assert_eq!(
        fresh.decide(Ok(JOY), &edit, 40),
        Write::Save { backup: true }
    );
}

#[cfg(unix)]
#[test]
fn a_kf_reached_through_a_symbolic_link_is_found() {
    use setlists::workspace::first_link;
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(vault.join("Libraries/Christmas/Joy")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("Joy.kf"), JOY).unwrap();
    // The file a link out of the vault.
    std::os::unix::fs::symlink(
        outside.join("Joy.kf"),
        vault.join("Libraries/Christmas/Joy/Joy.kf"),
    )
    .unwrap();
    assert_eq!(
        first_link(&vault, "Libraries/Christmas/Joy/Joy.kf")
            .unwrap()
            .as_deref(),
        Some("Libraries/Christmas/Joy/Joy.kf")
    );
    // A folder on the way a link.
    std::os::unix::fs::symlink(&outside, vault.join("Libraries/Linked")).unwrap();
    assert_eq!(
        first_link(&vault, "Libraries/Linked/Joy.kf")
            .unwrap()
            .as_deref(),
        Some("Libraries/Linked")
    );
    // A plain file: none.
    std::fs::write(vault.join("Libraries/Christmas/Plain.kf"), JOY).unwrap();
    assert_eq!(
        first_link(&vault, "Libraries/Christmas/Plain.kf").unwrap(),
        None
    );
}
