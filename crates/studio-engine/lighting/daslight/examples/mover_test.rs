//! A small showfile for the movers alone (11ch), to test them on their
//! own: at the mobile rig's addresses (201, 212, 223, 234), or with
//! `gearbox`, where they were in the Gearbox show (400, 411, 422, 433).
//!
//! ```sh
//! cargo run -p ignition-daslight --example mover_test -- "in.dvc" "out.dvc"
//! ```

use std::path::Path;

use ignition_daslight::profile::channels;
use ignition_daslight::{Error, Scene, Showfile, Step};

/// Where the movers are, by default the mobile rig's addresses; pass
/// `gearbox` as a third argument for the Gearbox show's.
const RIG: [(&str, u16); 4] = [
    ("Mover 1", 201),
    ("Mover 2", 212),
    ("Mover 3", 223),
    ("Mover 4", 234),
];
const GEARBOX: [(&str, u16); 4] = [
    ("Mover 1", 400),
    ("Mover 2", 411),
    ("Mover 3", 422),
    ("Mover 4", 433),
];

fn scene(name: &str, values: &[(String, Vec<Option<u8>>)]) -> Scene {
    let mut s = Scene::new(name, "#ff40a0ff");
    s.fixtures = values.to_vec();
    s
}

fn main() -> Result<(), Error> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        return Err(Error::Patch(
            "usage: mover_test <in.dvc> <out.dvc>".to_owned(),
        ));
    };
    let at = if args.get(3).map(String::as_str) == Some("gearbox") {
        GEARBOX
    } else {
        RIG
    };
    let mut show = Showfile::open(Path::new(input))?;
    let mut patch = show.patch()?;
    patch
        .fixtures
        .retain(|f| at.iter().any(|(n, _)| *n == f.name));
    for f in &mut patch.fixtures {
        if let Some((_, a)) = at.iter().find(|(n, _)| *n == f.name) {
            f.address = *a;
        }
    }
    let used: Vec<String> = patch.fixtures.iter().map(|f| f.profile.clone()).collect();
    patch.profiles.retain(|p| {
        p.get("SSLFIXUID")
            .is_some_and(|u| used.iter().any(|x| x == u))
    });
    show.set_patch(&patch)?;
    show.set_groups(&[(
        "Movers",
        patch.fixtures.iter().map(|f| f.uid.clone()).collect(),
    )]);
    show.clear_programming(&[]);

    let count = patch
        .fixtures
        .first()
        .and_then(|f| patch.profile(&f.profile))
        .map_or(0, |p| channels(p).len());
    // Gearbox's own mover scenes: pan, µpan, tilt, µtilt, colour, gobo,
    // shutter, dimmer, speed, function, dimmer mode.
    let open = |pan: u8, tilt: u8| -> Vec<Option<u8>> {
        let mut v = vec![Some(0_u8); count];
        for (i, x) in [(0, pan), (2, tilt), (6, 255), (7, 255)] {
            if let Some(s) = v.get_mut(i) {
                *s = Some(x);
            }
        }
        v
    };
    let all = |values: &Vec<Option<u8>>| -> Vec<(String, Vec<Option<u8>>)> {
        patch
            .fixtures
            .iter()
            .map(|f| (f.uid.clone(), values.clone()))
            .collect()
    };
    let mut test = vec![
        scene("ALL OPEN CENTRE", &all(&open(128, 128))),
        scene("ALL OPEN LEFT", &all(&open(64, 128))),
        scene("ALL OPEN RIGHT", &all(&open(192, 128))),
        scene("ALL OFF", &all(&vec![Some(0); count])),
    ];
    for f in &patch.fixtures {
        test.push(scene(
            &format!("{} @{}", f.name, f.address),
            &[(f.uid.clone(), open(128, 128))],
        ));
    }
    // Moving: steps the desk fades between and loops. Ticks are 1/25 s.
    let moving = |name: &str, points: &[(u8, u8)], fade: u32| -> Scene {
        let mut s = Scene::new(name, "#ffff40c0");
        s.steps = points
            .iter()
            .map(|(p, t)| Step {
                wait: 0,
                fade,
                fixtures: all(&open(*p, *t)),
            })
            .collect();
        s
    };
    test.push(moving("SWEEP PAN SLOW", &[(40, 128), (215, 128)], 75));
    test.push(moving("SWEEP PAN FAST", &[(40, 128), (215, 128)], 20));
    test.push(moving("SWEEP TILT", &[(128, 40), (128, 215)], 50));
    test.push(moving(
        "CIRCLE",
        &[
            (128, 60),
            (176, 80),
            (196, 128),
            (176, 176),
            (128, 196),
            (80, 176),
            (60, 128),
            (80, 80),
        ],
        10,
    ));
    show.add_bank("MOVER TEST", "#ff40a0ff", &test, &patch)?;

    let mut per_channel = vec![scene("ALL 0", &all(&vec![Some(0); count]))];
    for i in 0..count {
        for v in [255_u8, 128] {
            let mut values = vec![Some(0_u8); count];
            if let Some(s) = values.get_mut(i) {
                *s = Some(v);
            }
            per_channel.push(scene(
                &format!("CH{} {v}", i.saturating_add(1)),
                &all(&values),
            ));
        }
    }
    per_channel.push(scene("ALL 255", &all(&vec![Some(255); count])));
    show.add_bank("MOVER CHANNELS", "#ff40a0ff", &per_channel, &patch)?;
    show.save(Path::new(output))?;
    println!(
        "{} movers, {} + {} buttons -> {output}",
        patch.fixtures.len(),
        test.len(),
        per_channel.len()
    );
    Ok(())
}
