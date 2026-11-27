//! The leader's transport, as the entry a room follows: a member's
//! follower (session-sync's `TransportSync`) turns it into what its own
//! engine must do.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test fails loudly"
)]

use room::lead;
use session_sync::transport::{Command, LocalTransport, TransportSync};

fn at(song: &str, playing: bool, position: f64) -> LocalTransport {
    LocalTransport {
        playing,
        position,
        song: Some(song.to_owned()),
    }
}

#[test]
fn a_member_plays_stops_and_changes_song_with_the_leader() {
    let mut member = TransportSync::new("member");
    let here = at("1", false, 0.0);

    // The leader is stopped at the top of song 1: news, the first time.
    let first = lead::entry(None, &at("1", false, 0.0), 1_000.0, "leader").unwrap();
    assert!(member.remote(first.clone()));
    assert!(
        member.tick(&here, 1_000.0).commands.is_empty(),
        "already there"
    );

    // Nothing moved: nothing new to say.
    assert_eq!(
        lead::entry(Some(&first), &at("1", false, 0.0), 2_000.0, "leader"),
        None
    );

    // The leader plays from 10 s.
    let playing = lead::entry(Some(&first), &at("1", true, 10.0), 3_000.0, "leader").unwrap();
    member.remote(playing.clone());
    let told = member.tick(&here, 3_500.0).commands;
    assert_eq!(told.len(), 1, "{told:?}");
    let Command::Play { from } = told[0] else {
        panic!("{told:?}")
    };
    assert!((from - 10.5).abs() < 1e-9, "half a second on: {from}");

    // Playing along, the leader says nothing more…
    assert_eq!(
        lead::entry(Some(&playing), &at("1", true, 12.0), 5_000.0, "leader"),
        None
    );
    // …until it jumps.
    let jumped = lead::entry(Some(&playing), &at("1", true, 40.0), 5_000.0, "leader").unwrap();
    assert!(jumped.seq > playing.seq);

    // Another song: the member is asked to switch to it first.
    let next = lead::entry(Some(&jumped), &at("2", false, 0.0), 6_000.0, "leader").unwrap();
    member.remote(next);
    let told = member.tick(&at("1", true, 41.0), 6_200.0).commands;
    assert_eq!(told, vec![Command::SwitchSong("2".to_owned())]);
}
