//! Room 138's patch is the one the myDMX showfile carries
//! (`data/venues/room138-cbu/daslight/`): the towers in the ZQ01334's
//! 33ch mode, each lamp on its own warm white and its own colour cell.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::arithmetic_side_effects,
    reason = "integration test — see docs/ops/clippy.md"
)]

use ignition_core::Attribute;
use ignition_proto::ColorChannel;
use ignition_viz::venue::Venue;

fn colour(c: ColorChannel) -> Attribute {
    Attribute::ColorAdd { channel: c }
}

#[test]
fn every_tower_lamp_has_its_own_white_and_colour() {
    let venue = Venue::load("../data/venues/room138-cbu").expect("venue");
    let patch = venue.patch();
    let by_chan = |c: u32| patch.iter().find(|(chan, _)| *chan == c).map(|(_, f)| f).expect("patched");
    for (tower, start) in [(0_u32, 1_u16), (1, 34), (2, 67), (3, 100)] {
        for lamp in 1..=6_u16 {
            let f = by_chan(tower * 6 + u32::from(lamp));
            assert_eq!(f.address.start_channel, start, "tower {tower} lamp {lamp}");
            // 0 dimmer, 1 strobe, 2-7 warm white 1-6, 8-10 bar-wide RGB,
            // 11-14 macros and their speeds, then R G B per cell from 15.
            assert_eq!(f.map.offset_of(&colour(ColorChannel::White)), Some(1 + lamp), "white");
            assert_eq!(f.map.offset_of(&colour(ColorChannel::Red)), Some(12 + 3 * lamp), "red");
            assert_eq!(f.map.offset_of(&colour(ColorChannel::Blue)), Some(14 + 3 * lamp), "blue");
            assert_eq!(f.map.offset_of(&Attribute::Dimmer), Some(0), "dimmer");
        }
        // The tower itself: level, strobe and the bar-wide colour.
        let whole = by_chan(41 + tower);
        assert_eq!(whole.map.offset_of(&colour(ColorChannel::Red)), Some(8));
        assert_eq!(whole.map.offset_of(&colour(ColorChannel::White)), None);
    }
}

#[test]
fn the_rest_of_the_rig_is_where_the_address_list_says() {
    let venue = Venue::load("../data/venues/room138-cbu").expect("venue");
    let patch = venue.patch();
    let start = |c: u32| patch.iter().find(|(chan, _)| *chan == c).map(|(_, f)| f.address.start_channel);
    let pars: Vec<_> = (27..=34).map(start).collect();
    assert_eq!(pars, [141, 148, 155, 162, 169, 176, 183, 190].map(Some));
    let movers: Vec<_> = (35..=38).map(start).collect();
    assert_eq!(movers, [201, 212, 223, 234].map(Some));
    // Two Solena bars on the back wall; four RockStrips across the front.
    assert_eq!([start(45), start(53)], [301, 325].map(Some));
    let strips: Vec<_> = (77..=80).map(start).collect();
    assert_eq!(strips, [401, 429, 457, 485].map(Some));
}
