use super::*;

#[test]
fn the_session_builds_the_same_twice() {
    let (a, b) = (song(), song());
    assert_eq!(a.project, b.project);
    assert_eq!(a.waves, b.waves);
    assert_eq!(a.shut, b.shut);
    assert_eq!(a.meters.levels, b.meters.levels);
}

#[test]
fn every_track_a_shot_frames_is_in_the_session() {
    let golden = song();
    for shot in daw_theme::gallery::SHOTS {
        if let Some(name) = shot.frame.first {
            assert!(
                golden.track(name).is_some(),
                "{}: no track `{name}`",
                shot.name
            );
        }
    }
    assert_eq!(daw_theme::gallery::SESSION, VERSION);
}

#[test]
fn folders_nest_three_deep_and_close() {
    let golden = song();
    let depth: i32 = golden.project.tracks.iter().map(|t| t.folder_depth).sum();
    assert_eq!(depth, 0, "every folder the tracks open, they close");
    let deepest = golden
        .project
        .tracks
        .iter()
        .map(|t| t.guid.matches('/').count())
        .max();
    assert!(deepest >= Some(3), "a track three folders down");
}

#[test]
fn every_clip_colour_and_every_clip_state_is_there() {
    let golden = song();
    let items: Vec<&daw_proto::Item> = golden.project.items.values().flatten().collect();
    for colour in tracks::COLOURS {
        assert!(
            items.iter().any(|i| i.color == Some(colour)),
            "{colour:06x}"
        );
    }
    assert!(items.iter().any(|i| i.selected));
    assert!(items.iter().any(|i| i.muted));
    assert!(items.iter().any(|i| i.locked));
    assert!(items.iter().any(|i| i.fade_in_length.as_seconds() > 0.0));
    assert!(items.iter().any(|i| i.fixed_lane == Some(2)));
    assert!(golden.meters.levels.iter().any(|l| l.peak_left >= 1.0));
    assert!(!golden.melody.snapshot.ccs.is_empty());
    assert!(golden.drums.snapshot.notes.iter().any(|n| n.selected));
    assert!(!golden.signal.is_empty(), "the kick's Signal chains");
}
