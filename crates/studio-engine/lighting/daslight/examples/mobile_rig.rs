//! Builds a venue's myDMX 5 showfile and its address list from the venue
//! itself (`fixtures.json`), so the desk and Ignition patch the same rig
//! under the same names, at the same addresses, in the same places.
//!
//! ```sh
//! cargo run -p ignition-daslight --example mobile_rig -- \
//!     features/lighting/daslight/tests/data/mobile-lighting.dvc \
//!     features/lighting/data/venues/room138-cbu
//! ```
//!
//! The first argument is a showfile from the desk: it supplies the
//! interface settings and the desk's own par and ZQ01334 profiles. The output goes to the venue's `daslight/` folder.
//!
//! A third argument, the JSON `ignition-viz`'s `cook_dmx` example records
//! (every profile look and library effect played on this venue), fills
//! the LOOKS and effect banks with Steps scenes that send what Ignition
//! sends, named as in Ignition, and lists them in `COOKED.md`:
//!
//! ```sh
//! cd features/lighting   # the cook finds the profile under data/profiles
//! cargo run -p ignition-viz --example cook_dmx -- data/venues/room138-cbu /tmp/cooked.json
//! cargo run -p ignition-daslight --example mobile_rig -- \
//!     daslight/tests/data/mobile-lighting.dvc data/venues/room138-cbu /tmp/cooked.json
//! ```
//!
//! Two more arguments add a song's show: `cook_song`'s recording of its
//! cues and the venue's audio plan for it (a third, the song's hits, adds
//! a flash block per hit). Re-run all of it when the song's cue list or
//! hits change:
//!
//! ```sh
//! cargo run -p ignition-viz --example cook_song -- data/venues/room138-cbu \
//!     data/songs/jt-recital/out-of-the-black.json 159 /tmp/song.json
//! cargo run -p ignition-daslight --example mobile_rig -- \
//!     daslight/tests/data/mobile-lighting.dvc data/venues/room138-cbu /tmp/cooked.json \
//!     /tmp/song.json data/venues/room138-cbu/daslight/out-of-the-black.audio.json \
//!     [data/songs/jt-recital/out-of-the-black.hits.json]
//! ```

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::suboptimal_flops,
    reason = "quaternion and axis maths read best as w, x, y, z"
)]

#[path = "support/song_show.rs"]
mod song_show;
#[path = "support/scenery.rs"]
mod scenery;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ignition_daslight::easyview::{self, Placement, Room, rotation_towards};
use ignition_daslight::library::{
    mini_gobo_moving_head_11ch, rockstrip_252_28ch, solena_max_bar_24ch, zq01334_33ch,
};
use ignition_daslight::cook::{self, Motion, find_cycle, lit_and_moving, motion_of};
use ignition_daslight::fixups::Rig;
use ignition_daslight::fx::{self, Beam, ColorEffect, MoveEffect};
use ignition_daslight::profile::{self as profiles, channels, cells};
use ignition_daslight::xml::Element;
use ignition_daslight::{Error, Fixture, Patch, Scene, Showfile};

/// How each fixture type is set on its own menu.
struct Kind {
    label: &'static str,
    mode: &'static str,
    /// Channels kept free after it, so a bigger mode later moves nothing.
    reserve: u16,
}

const TOWER: Kind = Kind {
    label: "U'King ZQ01334 tower",
    mode: "33CH",
    reserve: 33,
};
const PAR: Kind = Kind {
    label: "U'King 36 LED par",
    mode: "7CH (DMX mode; its only one)",
    reserve: 7,
};
const MOVER: Kind = Kind {
    label: "Mini Gobo moving head",
    mode: "11CH",
    reserve: 11,
};
const SOLENA: Kind = Kind {
    label: "Solena Max Bar 28 RGB",
    mode: "24CH",
    reserve: 24,
};
const ROCK: Kind = Kind {
    label: "Rockville RockStrip 252",
    mode: "28CH",
    reserve: 28,
};

/// One record of the venue's `fixtures.json`: a fixture, or one lamp of a
/// several-lamp fixture (`cell`).
struct Record {
    name: String,
    /// `wired_as` when set (the fixture the address follows), else `model`.
    wired: String,
    universe: u16,
    address: u16,
    cell: Option<u64>,
    /// Metres: x stage right (-) to left (+), y downstage (-) to upstage (+),
    /// z up.
    x: f64,
    y: f64,
    z: f64,
    /// The hang, `[w, x, y, z]`; the beam is the fixture's -Z.
    quat: [f64; 4],
}

fn read_venue(dir: &Path) -> Result<Vec<Record>, Error> {
    let path = dir.join("fixtures.json");
    let text =
        std::fs::read_to_string(&path).map_err(|e| Error::Io(path.display().to_string(), e))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| Error::Xml(e.to_string()))?;
    let list = json
        .as_array()
        .ok_or_else(|| Error::Patch("fixtures.json is not a list".into()))?;
    let text_of = |v: &serde_json::Value, k: &str| {
        v.get(k)
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_owned()
    };
    let num = |v: &serde_json::Value, k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u16::try_from(n).ok())
    };
    Ok(list
        .iter()
        .filter(|r| {
            r.get("patched")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true)
        })
        .filter_map(|r| {
            let wired = text_of(r, "wired_as");
            Some(Record {
                name: text_of(r, "name"),
                wired: if wired.is_empty() {
                    text_of(r, "model")
                } else {
                    wired
                },
                universe: num(r, "universe").unwrap_or(1),
                address: num(r, "address")?,
                cell: r.get("cell").and_then(serde_json::Value::as_u64),
                x: r.pointer("/position/x")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
                y: r.pointer("/position/y")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
                z: r.pointer("/position/z")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0),
                quat: ["w", "x", "y", "z"].map(|k| {
                    r.pointer(&format!("/quat/{k}"))
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(if k == "w" { 1.0 } else { 0.0 })
                }),
            })
        })
        .collect())
}

/// The desk's name for a fixture: the record that is the fixture itself
/// (cell 0, or the only one), else its first lamp's name without the lamp.
fn fixture_name(lamps: &[&Record]) -> String {
    let whole = lamps
        .iter()
        .find(|r| r.cell.is_none_or(|c| c == 0))
        .or_else(|| lamps.first());
    let name = whole.map(|r| r.name.as_str()).unwrap_or_default();
    let name = if lamps.len() > 1 && whole.is_some_and(|r| r.cell.is_some_and(|c| c > 0)) {
        name.split(" \u{b7} ").next().unwrap_or(name)
    } else {
        name
    };
    // The desk's fonts are not trusted with a dash.
    name.replace(" \u{2014} ", " - ")
}

#[allow(clippy::too_many_lines, reason = "one straight-line build of the showfile, step by step")]
fn main() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let (Some(template), Some(venue)) = (args.next(), args.next()) else {
        return Err(Error::Patch(
            "usage: mobile_rig TEMPLATE.dvc VENUE_DIR [COOKED.json]".into(),
        ));
    };
    let cooked = args.next().map(PathBuf::from);
    let venue = PathBuf::from(venue);
    let out = venue.join("daslight");
    std::fs::create_dir_all(&out).map_err(|e| Error::Io(out.display().to_string(), e))?;

    let mut show = Showfile::open(&PathBuf::from(&template))?;
    let desk = show.patch()?;
    let need = |name: &str| {
        desk.profile_named(name)
            .cloned()
            .ok_or_else(|| Error::Patch(format!("template has no {name}")))
    };
    let zq = zq01334_33ch(&need("UKing/ZQ01334.ssl2")?);
    let par = need("UKing/U'King 36 LED RGB 7 Channel Par.ssl2")?;
    let mover = mini_gobo_moving_head_11ch()?;
    let solena = solena_max_bar_24ch();
    let rock = rockstrip_252_28ch();
    let uid = |p: &Element| p.get("SSLFIXUID").unwrap_or_default().to_owned();
    // What the venue wires each type as, and the desk profile for it.
    let types: [(&str, &Kind, String); 5] = [
        ("ZQ01334 33ch", &TOWER, uid(&zq)),
        ("36 LED Par Can", &PAR, uid(&par)),
        ("Mini Gobo Moving Head Light", &MOVER, uid(&mover)),
        ("Solena Max Bar 28 RGB 24ch", &SOLENA, uid(&solena)),
        ("RockStrip 252", &ROCK, uid(&rock)),
    ];

    let records = read_venue(&venue)?;
    let mut by_address: BTreeMap<(u16, u16), Vec<&Record>> = BTreeMap::new();
    for r in &records {
        by_address
            .entry((r.universe, r.address))
            .or_default()
            .push(r);
    }
    let mut patch = Patch::default();
    let mut rows: Vec<(&Kind, String, u16, u16)> = Vec::new();
    let mut placements: Vec<Placement> = Vec::new();
    for ((universe, address), lamps) in &by_address {
        let wired = lamps.first().map(|r| r.wired.as_str()).unwrap_or_default();
        let (_, kind, profile) = types
            .iter()
            .find(|(key, _, _)| wired.contains(key))
            .ok_or_else(|| Error::Patch(format!("no desk profile for {wired} at {address}")))?;
        let n = lamps.len();
        let count = f64::from(u32::try_from(n).unwrap_or(1));
        let (x, y) = lamps
            .iter()
            .fold((0.0, 0.0), |(x, y), r| (x + r.x / count, y + r.y / count));
        let name = fixture_name(lamps);
        let (lowest, highest) = lamps.iter().fold((f64::MAX, f64::MIN), |(lo, hi), r| {
            (lo.min(r.z), hi.max(r.z))
        });
        let (px, py, angle) = front_view(kind, x, lowest);
        let mut f = Fixture::named(&name, profile, *address).at(px, py, angle);
        f.universe = *universe;
        // The middle of the fixture: a tower's is halfway up its stack.
        let z = f64::midpoint(lowest, highest);
        let aim = lamps
            .iter()
            .find(|r| r.cell.is_none_or(|c| c == 0))
            .or_else(|| lamps.first())
            .map_or([0.0, 0.0, -1.0], |r| beam(r.quat));
        placements.push(Placement {
            uid: f.uid.clone(),
            position: scenery::ev([x, y, z]),
            rotation: hang(aim, kind),
        });
        patch.fixtures.push(f);
        rows.push((kind, name, *address, kind.reserve));
    }
    for p in [zq, par, mover, solena, rock] {
        patch.add_profile(p);
    }
    patch.check()?;

    let ids = |prefix: &str| -> Vec<String> {
        patch
            .fixtures
            .iter()
            .filter(|f| f.name.starts_with(prefix))
            .map(|f| f.uid.clone())
            .collect()
    };
    let all: Vec<String> = patch.fixtures.iter().map(|f| f.uid.clone()).collect();
    let mut bars = ids("Wall Bar");
    bars.extend(ids("RockStrip"));
    show.set_patch(&patch)?;
    show.set_groups(&[
        ("All", all),
        ("Towers", ids("Tower")),
        ("Tree 1", ids("Tree 1")),
        ("Tree 2", ids("Tree 2")),
        ("Pars", ids("Tree")),
        ("Movers", ids("Mover")),
        ("Wall Bars", ids("Wall Bar")),
        ("RockStrips", ids("RockStrip")),
        ("Bars", bars),
    ]);
    show.clear_programming(&[]);
    let mut banks: BTreeMap<&str, Vec<Scene>> = BTreeMap::new();
    if let Some(path) = &cooked {
        let (by_bank, report) = cooked_banks(path, &patch)?;
        banks = by_bank;
        let dest = out.join("COOKED.md");
        std::fs::write(&dest, report).map_err(|e| Error::Io(dest.display().to_string(), e))?;
    }
    for (bank, color) in BANKS {
        let scenes = banks.remove(bank).unwrap_or_default();
        show.add_bank(bank, color, &scenes, &patch)?;
    }
    // The desk's own effects come with the cooked ones: a patch-only
    // showfile carries no effect racks.
    if cooked.is_some() {
        show.add_bank("NATIVE", "#ff00c0ff", &native_bank(&patch), &patch)?;
    }
    show.add_bank("TEST", "#ffffb300", &test_bank(&patch)?, &patch)?;
    show.add_bank("ROCKSTRIP CHANNELS", "#ffff00ff", &rockstrip_channels(&patch), &patch)?;
    for (bank, scenes) in tower_lamp_banks(&patch) {
        show.add_bank(&bank, "#ffff8000", &scenes, &patch)?;
    }
    // A song's show: its cues as scenes and a Super Scene that plays them
    // against its audio (support/song_show.rs).
    if let (Some(recording), Some(plan)) = (args.next(), args.next()) {
        let hits = args.next().map(PathBuf::from);
        let report = song_show::add(
            &mut show,
            &patch,
            Path::new(&recording),
            Path::new(&plan),
            hits.as_deref(),
        )?;
        let dest = out.join("SHOWS.md");
        std::fs::write(&dest, report).map_err(|e| Error::Io(dest.display().to_string(), e))?;
    }

    let dvc = out.join("Mobile Lighting 33ch.dvc");
    show.save(&dvc)?;
    // Prove the written file reads back to the same patch and scenes,
    // and writes out again unchanged.
    let reread = Showfile::open(&dvc)?;
    let back = reread.patch()?;
    if back.fixtures != patch.fixtures
        || reread.scene_names() != show.scene_names()
        || reread.to_text() != show.to_text()
    {
        return Err(Error::Patch("written showfile does not read back".into()));
    }
    for (bank, scenes) in show.scene_names() {
        println!("{bank:16} {} scenes", scenes.len());
    }

    write_report(&out, &patch, &rows)?;
    write_visualizer(&venue, &out, &patch, &placements)?;
    println!("{} fixtures -> {}", patch.fixtures.len(), dvc.display());
    Ok(())
}

/// Where a fixture sits on the desk's 2D view, `(POSX, POSY, ANGLE)`:
/// the rig seen from the audience, so the desk's pixel effects (Color FX,
/// Mappings, Color Mappings), which run over the cells' places there,
/// sweep the stage as Ignition's do. Audience-left on the left, up the
/// page is up the rig, 200 units a metre across and up; `x` is the
/// fixture's middle across the stage, `z` its lowest lamp.
///
/// The desk draws a cell 30 units square, so a picture true to scale
/// would overlap a tower's lamps (21 cm apart) and a bar's cells (14 cm);
/// the rows are spread instead, top to bottom: the trees' pars (2.5 m),
/// the movers (lifted 20 cm clear of the towers they sit on), the towers'
/// lamps as columns, the wall bars along the band wall's foot and the
/// `RockStrips` downstage of them as rows of pixels.
///
/// A tower is its profile's six-by-two cell grid (warm-white COBs, then
/// the RGB rings) turned a quarter: ANGLE 270 runs it up the page, lamp 1
/// at the bottom as on the stand, its COBs in the left column and its
/// rings in the right. The desk stores a turned fixture's cells unturned
/// and turns them about the first cell, as it wrote the Gearbox show's
/// towers (ANGLE 90).
fn front_view(kind: &Kind, x: f64, z: f64) -> (i32, i32, i32) {
    const PER_METRE: f64 = 200.0;
    const CELL: f64 = 30.0;
    let units = |v: f64| -> i32 { format!("{}", v.round()).parse().unwrap_or(0) };
    // Cells across the page, so the fixture centres on `x`.
    let (across, lift, angle) = match kind.mode {
        "33CH" => (2.0, 0.0, 270),
        "11CH" => (2.0, 0.2, 0),
        "24CH" | "28CH" => (8.0, 0.0, 0),
        _ => (1.0, 0.0, 0),
    };
    // The RockStrips lie downstage of the wall bars at the same height:
    // a row below them.
    let below = if kind.mode == "28CH" { 2.0 * CELL + 20.0 } else { 0.0 };
    let px = x.mul_add(PER_METRE, -across * CELL / 2.0);
    let py = (-(z + lift)).mul_add(PER_METRE, below - CELL / 2.0);
    (units(px), units(py), angle)
}

/// The direction a fixture's beam leaves it: its local -Z turned by its
/// hang quaternion `[w, x, y, z]`.
fn beam(q: [f64; 4]) -> [f64; 3] {
    let [w, x, y, z] = q;
    // R * (0, 0, -1): minus the rotation matrix's third column.
    [
        -2.0 * (x * z + w * y),
        -2.0 * (y * z - w * x),
        -(1.0 - 2.0 * (x * x + y * y)),
    ]
}

/// EasyView's rotation for a fixture throwing along `aim` (Ignition axes).
/// A tower is a bar stood on end, so its cells run upward: rolled 90
/// degrees about its beam.
fn hang(aim: [f64; 3], kind: &Kind) -> [f64; 3] {
    let mut r = rotation_towards([aim[0], aim[2], aim[1]]);
    if kind.mode == "33CH" {
        r[2] = 90.0;
    }
    r
}

/// The EasyView project, and the profiles as library files for the
/// desk's `ScanLibrary` folder.
fn write_visualizer(
    venue: &Path,
    out: &Path,
    patch: &Patch,
    placements: &[Placement],
) -> Result<(), Error> {
    // Room 138: 14.00 m by 10.12 m, 3.50 m to the soffits, 5.45 m to the
    // ceiling's apex; and what is in it, from the venue's own files.
    let room = Room {
        width: 1400.0,
        depth: 1012.0,
        height: 545.0,
    };
    let ev2 = easyview::project_in(patch, room, scenery::scenery(venue)?, placements)?;
    let path = out.join("Mobile Lighting 33ch.ev2");
    std::fs::write(&path, easyview::to_text(&ev2))
        .map_err(|e| Error::Io(path.display().to_string(), e))?;
    for lib in &patch.profiles {
        let name = lib.get("SSLNAME").unwrap_or_default();
        let path = out.join("ScanLibrary").join(name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Io(dir.display().to_string(), e))?;
        }
        std::fs::write(&path, easyview::library_file(lib))
            .map_err(|e| Error::Io(path.display().to_string(), e))?;
    }
    Ok(())
}

/// Left to right on a tower or bar: red, orange, yellow, green, cyan, blue,
/// purple, magenta. A cell out of order or missing shows at a glance.
const RAINBOW: [[u8; 3]; 8] = [
    [255, 0, 0],
    [255, 96, 0],
    [255, 220, 0],
    [0, 255, 0],
    [0, 255, 255],
    [0, 0, 255],
    [128, 0, 255],
    [255, 0, 160],
];

/// What one fixture shows in its test scene, by profile: full on, every
/// macro and strobe off, multi-cell fixtures in the rainbow.
fn test_values(profile: &str) -> Option<Vec<Option<u8>>> {
    let rainbow = |cells: usize| RAINBOW.iter().take(cells).flatten().copied();
    let v: Vec<u8> = match profile {
        // Dimmer, strobe off, WW 1-6 full, global RGB 0, four macro channels
        // off, then R G B of cells 1-6.
        "UKing/ZQ01334 33CH.ssl2" => [255, 0]
            .into_iter()
            .chain([255; 6])
            .chain([0; 7])
            .chain(rainbow(6))
            .collect(),
        // Dimmer, R, G, B, strobe off, manual mode, speed.
        "UKing/U'King 36 LED RGB 7 Channel Par.ssl2" => vec![255, 255, 255, 255, 0, 0, 0],
        // Eight cells of R, G, B, then dimmer full, strobe off, no chase,
        // dimmer mode.
        "Rockville/Rockville Rockstrip 252 28CH.ssl2" => {
            rainbow(8).chain([255, 0, 0, 0]).collect()
        }
        "Solena/Max Bar 28 RGB 24CH.ssl2" => rainbow(8).collect(),
        // X, µX, Y, µY centred; white, open gobo; shutter open (255, as
        // the desk's own mover scenes set it); dimmer full; fastest; no
        // function or dimmer mode.
        "_varied/Mini Gobo Moving Head Light.ssl2" => vec![128, 0, 128, 0, 0, 0, 255, 255, 0, 0, 0],
        _ => return None,
    };
    Some(v.into_iter().map(Some).collect())
}

/// One scene per light, in address order, then everything at once.
fn test_bank(patch: &Patch) -> Result<Vec<Scene>, Error> {
    let mut all = Scene::new("ALL", "#ffffffff");
    let mut scenes = Vec::new();
    let mut fixtures: Vec<&Fixture> = patch.fixtures.iter().collect();
    fixtures.sort_by_key(|f| f.address);
    for (i, f) in fixtures.into_iter().enumerate() {
        let name = patch
            .profile(&f.profile)
            .and_then(|p| p.get("SSLNAME"))
            .unwrap_or_default();
        let values =
            test_values(name).ok_or_else(|| Error::Patch(format!("no test values for {name}")))?;
        let mut scene = Scene::new(
            &format!("{:02} {} @{}", i.saturating_add(1), f.name, f.address),
            "#ffffb300",
        );
        scene.fixtures.push((f.uid.clone(), values.clone()));
        all.fixtures.push((f.uid.clone(), values));
        scenes.push(scene);
    }
    scenes.push(all);
    Ok(scenes)
}

/// RockStrip 1's channels 1-24 one at a time (master dimmer full, dimmer
/// mode), so what each lights on the real strip shows which layout it is.
fn rockstrip_channels(patch: &Patch) -> Vec<Scene> {
    let Some(strip) = patch.fixtures.iter().find(|f| f.name == "RockStrip 1") else {
        return Vec::new();
    };
    (0..24_usize)
        .map(|ch| {
            let mut v = vec![Some(0_u8); 28];
            for i in [ch, 24] {
                if let Some(c) = v.get_mut(i) {
                    *c = Some(255);
                }
            }
            let mut s = Scene::new(&format!("CH{}", ch.saturating_add(1)), "#ffff00ff");
            s.fixtures.push((strip.uid.clone(), v));
            s
        })
        .collect()
}

/// A bank per tower: every lamp's warm white, red, green and blue on its
/// own, then each of those on all six lamps, then the bar-wide RGB.
/// Which lamp is which on the real fixture is what these find out.
fn tower_lamp_banks(patch: &Patch) -> Vec<(String, Vec<Scene>)> {
    // 33ch: 0 dimmer, 1 strobe, 2-7 warm white 1-6, 8-10 bar-wide RGB,
    // 11-14 macros and speeds, 15.. R G B per lamp.
    const WARM: usize = 2;
    const BAR_RGB: usize = 8;
    const CELLS: usize = 15;
    let colours = [
        ("RED", 0_usize, "#ffff0000"),
        ("GREEN", 1, "#ff00ff00"),
        ("BLUE", 2, "#ff0060ff"),
    ];
    let only = |on: &[usize]| -> Vec<Option<u8>> {
        let mut v = vec![Some(0_u8); 33];
        if let Some(d) = v.get_mut(0) {
            *d = Some(255);
        }
        for i in on {
            if let Some(c) = v.get_mut(*i) {
                *c = Some(255);
            }
        }
        v
    };
    let mut towers: Vec<&Fixture> = patch
        .fixtures
        .iter()
        .filter(|f| {
            patch.profile(&f.profile).and_then(|p| p.get("SSLNAME"))
                == Some("UKing/ZQ01334 33CH.ssl2")
        })
        .collect();
    towers.sort_by_key(|f| f.address);
    let mut banks = Vec::new();
    for t in towers {
        let mut scenes = Vec::new();
        let mut add = |name: String, color: &str, on: Vec<usize>| {
            let mut s = Scene::new(&name, color);
            s.fixtures.push((t.uid.clone(), only(&on)));
            scenes.push(s);
        };
        for lamp in 0..6_usize {
            let n = lamp.saturating_add(1);
            add(
                format!("L{n} WARM"),
                "#ffffb300",
                vec![WARM.saturating_add(lamp)],
            );
            for (label, k, color) in colours {
                add(
                    format!("L{n} {label}"),
                    color,
                    vec![
                        CELLS
                            .saturating_add(lamp.saturating_mul(3))
                            .saturating_add(k),
                    ],
                );
            }
        }
        add(
            "ALL WARM".to_owned(),
            "#ffffb300",
            (0..6).map(|l| WARM.saturating_add(l)).collect(),
        );
        for (label, k, color) in colours {
            add(
                format!("ALL {label}"),
                color,
                (0..6_usize)
                    .map(|l| CELLS.saturating_add(l.saturating_mul(3)).saturating_add(k))
                    .collect(),
            );
        }
        add(
            "BAR RGB WHITE".to_owned(),
            "#ffffffff",
            vec![
                BAR_RGB,
                BAR_RGB.saturating_add(1),
                BAR_RGB.saturating_add(2),
            ],
        );
        banks.push((format!("{} LAMPS", t.name.to_uppercase()), scenes));
    }
    banks
}

/// The address list, as a table to set the lights from and as CSV.
fn write_report(
    out: &std::path::Path,
    patch: &Patch,
    rows: &[(&Kind, String, u16, u16)],
) -> Result<(), Error> {
    let mut csv = String::from("fixture,type,mode,address,last_channel,reserved_to\n");
    let mut md = String::from(
        "# Mobile rig: address list\n\nUniverse 1, on the myDMX 5 Express. Generated from this venue's \
         `fixtures.json` by `cargo run -p ignition-daslight --example mobile_rig`; the \
         showfile is `Mobile Lighting 33ch.dvc` next to this file.\n\n\
         | Fixture | Type | Set mode | Address | Uses | Free to |\n|---|---|---|---|---|---|\n",
    );
    for (kind, name, addr, reserve) in rows {
        let f = patch.fixtures.iter().find(|f| &f.name == name);
        let used = f.map_or(0, |f| patch.footprint(f));
        let last = usize::from(*addr).saturating_add(used).saturating_sub(1);
        let reserved = addr.saturating_add(*reserve).saturating_sub(1);
        let _ = writeln!(
            csv,
            "{name},{},{},{addr},{last},{reserved}",
            kind.label, kind.mode
        );
        let _ = writeln!(
            md,
            "| {name} | {} | {} | **{addr:03}** | {addr}-{last} | {reserved} |",
            kind.label, kind.mode
        );
    }
    md.push_str("\n## Channel layouts\n");
    for p in &patch.profiles {
        let _ = writeln!(md, "\n**{}**\n", p.get("SSLNAME").unwrap_or_default());
        for (i, c) in channels(p).iter().enumerate() {
            let _ = writeln!(md, "{}. {}", i.saturating_add(1), c.name);
        }
    }
    for (file, text) in [("addresses.csv", csv), ("ADDRESSES.md", md)] {
        let dest = out.join(file);
        std::fs::write(&dest, text).map_err(|e| Error::Io(dest.display().to_string(), e))?;
    }
    Ok(())
}

/// The programmed banks, in the order the desk shows them.
const BANKS: [(&str, &str); 7] = [
    ("LOOKS", "#ffffffff"),
    ("TOWERS", "#ffff8000"),
    ("PARS", "#ffff4040"),
    ("BARS", "#ff40ff40"),
    ("MOVERS", "#ff4080ff"),
    ("FX", "#ffc040ff"),
    ("STROBES", "#ffffff00"),
];

/// Frames skipped before looking for a cycle: two seconds at 10 fps, so a
/// look's fade-in is not taken for part of it.
const WARMUP_SECONDS: u16 = 2;

/// Off-by-one steps of the engine's arithmetic are still a repeat.
const TOLERANCE: u8 = 1;

/// What a fixture is, by its desk profile: the bank that drives it.
fn bank_of(patch: &Patch, f: &Fixture) -> &'static str {
    match patch.profile(&f.profile).and_then(|p| p.get("SSLNAME")) {
        Some("UKing/ZQ01334 33CH.ssl2") => "TOWERS",
        Some("UKing/U'King 36 LED RGB 7 Channel Par.ssl2") => "PARS",
        Some("_varied/Mini Gobo Moving Head Light.ssl2") => "MOVERS",
        _ => "BARS",
    }
}

/// Strobes, flashes, pops and stabs: hits, kept apart from beds.
fn is_strobe(name: &str) -> bool {
    ["strobe", "flash", "pop", "stab"]
        .iter()
        .any(|w| name.to_lowercase().contains(w))
}

/// One recorded item: its kind (`look`, `effect`), name and frames.
type Item = (String, String, Vec<Vec<u8>>);

/// The cook's JSON: `{fps, items: [{kind, name, frames: [[512 bytes]]}]}`.
fn read_cooked(path: &Path) -> Result<(u16, Vec<Item>), Error> {
    let text =
        std::fs::read_to_string(path).map_err(|e| Error::Io(path.display().to_string(), e))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| Error::Xml(e.to_string()))?;
    let fps = json
        .get("fps")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u16::try_from(n).ok())
        .ok_or_else(|| Error::Patch("cooked JSON has no fps".into()))?;
    let byte = |v: &serde_json::Value| v.as_u64().and_then(|n| u8::try_from(n).ok());
    let items = json
        .get("items")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|it| {
                    let text = |k: &str| {
                        it.get(k)
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    };
                    let frames = it
                        .get("frames")
                        .and_then(serde_json::Value::as_array)
                        .map(|fs| {
                            fs.iter()
                                .filter_map(serde_json::Value::as_array)
                                .map(|f| f.iter().map(|b| byte(b).unwrap_or(0)).collect())
                                .collect()
                        })
                        .unwrap_or_default();
                    (text("kind"), text("name"), frames)
                })
                .collect()
        })
        .unwrap_or_default();
    Ok((fps, items))
}

/// The frames of a recording that become its scene, the cycle found, and
/// whether it is a one-shot; `None` for an effect that never changes.
///
/// An effect that ends held is a one-shot (a hit, a fly-in): the whole of
/// it from the first frame, then where it lands held a second. One that
/// never changes does nothing this rig can show (a zoom or an iris on a
/// mover that has none).
fn span_of(frames: &[Vec<u8>], fps: u16, look: bool) -> Option<(Vec<Vec<u8>>, cook::Cycle, bool)> {
    let cycle = find_cycle(frames, usize::from(WARMUP_SECONDS.saturating_mul(fps)), TOLERANCE);
    if cycle.len == 1 && !look {
        let k = cook::settles_at(frames, TOLERANCE)?;
        let mut shot: Vec<Vec<u8>> = frames.iter().take(k.saturating_add(1)).cloned().collect();
        if let Some(rest) = shot.last().cloned() {
            shot.extend(std::iter::repeat_n(rest, usize::from(fps)));
        }
        return Some((shot, cycle, true));
    }
    let end = cycle.start.saturating_add(cycle.len);
    let span = frames.get(cycle.start..end).map(<[Vec<u8>]>::to_vec).unwrap_or_default();
    Some((span, cycle, false))
}

/// Every cooked look and effect as a scene, by bank, and a report of what
/// each became.
fn cooked_banks(
    path: &Path,
    patch: &Patch,
) -> Result<(BTreeMap<&'static str, Vec<Scene>>, String), Error> {
    let (fps, items) = read_cooked(path)?;
    let mut banks: BTreeMap<&'static str, Vec<Scene>> = BTreeMap::new();
    let mut report = format!(
        "# Cooked scenes\n\nIgnition's looks and effects as myDMX Steps scenes, recorded off \
         the engine at {fps} fps (`ignition-viz` example `cook_dmx`) and written by \
         `mobile_rig`. A cycle is where the output repeats after a {WARMUP_SECONDS} s \
         warm-up; *fitted* means the period falls between frames and the closest whole-frame \
         repeat was used. A *one-shot* (an effect that ends held: a hit, a fly-in) is cooked from \
         its first frame to where it lands, held a second, and loops on the desk like the rest. \
         RockStrip colour comes from the wall bars (`recolour_strips`). Step times are 1/25 s ticks.\n\n\
         | Bank | Scene | Ignition | Cycle | Steps | Moves by | Drives |\n|---|---|---|---|---|---|---|\n"
    );
    let all: Vec<&Fixture> = patch.fixtures.iter().filter(|f| f.universe == 1).collect();
    let rig = Rig::of(patch);
    let mut skipped = Vec::new();
    for (kind, name, frames) in &items {
        let frames = rig.recoloured(frames);
        let look = kind == "look";
        let Some((frames, cycle, one_shot)) = span_of(&frames, fps, look) else {
            skipped.push(format!("{kind} {name}: sends the same frame throughout on this venue"));
            continue;
        };
        if frames.is_empty() {
            skipped.push(format!("{kind} {name}: no frames"));
            continue;
        }
        let frames = frames.as_slice();
        let (lit, moving) = lit_and_moving(frames, patch, 1);
        let fixtures: Vec<&Fixture> = if look {
            all.clone()
        } else if moving.is_empty() {
            lit.clone()
        } else {
            moving.clone()
        };
        if fixtures.is_empty() {
            skipped.push(format!("{kind} {name}: lights nothing"));
            continue;
        }
        let mut drives: Vec<&'static str> = fixtures.iter().map(|f| bank_of(patch, f)).collect();
        drives.sort_unstable();
        drives.dedup();
        let bank = if look {
            "LOOKS"
        } else if is_strobe(name) {
            "STROBES"
        } else if let [one] = drives.as_slice() {
            one
        } else {
            "FX"
        };
        let motion = if is_strobe(name) {
            Motion::Snap
        } else {
            motion_of(&cook::coarse(frames, patch, 1))
        };
        let (_, color) = BANKS
            .iter()
            .find(|(b, _)| *b == bank)
            .copied()
            .unwrap_or(("", "#ff808080"));
        let scene = cook::scene_from_frames(name, color, frames, fps, motion, patch, &fixtures);
        let steps = scene.steps.len().max(1);
        let span = if one_shot {
            format!(
                "one-shot {:.1} s",
                f64::from(u32::try_from(frames.len()).unwrap_or(0)) / f64::from(fps)
            )
        } else if cycle.len == 1 {
            "static".to_owned()
        } else {
            format!(
                "{:.1} s{}",
                f64::from(u32::try_from(cycle.len).unwrap_or(0)) / f64::from(fps),
                if cycle.exact { "" } else { " fitted" }
            )
        };
        let moves = match (steps, motion) {
            (1, _) => "-",
            (_, Motion::Snap) => "snap",
            (_, Motion::Fade) => "fade",
        };
        let _ = writeln!(
            report,
            "| {bank} | {} | {kind} | {span} | {steps} | {moves} | {} |",
            scene.name,
            drives.join(", ").to_lowercase()
        );
        banks.entry(bank).or_default().push(scene);
    }
    if !skipped.is_empty() {
        report.push_str("\n## Skipped\n\n");
        for s in skipped {
            let _ = writeln!(report, "- {s}");
        }
    }
    Ok((banks, report))
}

/// Static values under a desk effect: dimmers up, strobes and modes off,
/// colour and position left to the effect. A Mini Gobo takes what the
/// desk's own Move FX scenes held (shutter open, dimmer full, speed 251).
fn under_effect(patch: &Patch, f: &Fixture) -> Vec<Option<u8>> {
    let Some(profile) = patch.profile(&f.profile) else {
        return Vec::new();
    };
    if profile.get("SSLNAME") == Some("_varied/Mini Gobo Moving Head Light.ssl2") {
        return [
            Some(0),
            None,
            Some(0),
            None,
            None,
            None,
            Some(255),
            Some(255),
            Some(251),
            None,
            None,
        ]
        .to_vec();
    }
    channels(profile)
        .iter()
        .map(|c| match c.kind {
            profiles::Kind::Dimmer => Some(255),
            profiles::Kind::Strobe | profiles::Kind::Macro | profiles::Kind::Speed | profiles::Kind::Other => {
                Some(0)
            }
            _ => None,
        })
        .collect()
}

/// One effect scene: the static values of every fixture the rack runs
/// over, and the rack.
fn effect_scene(name: &str, patch: &Patch, cells: &[Beam], rack: Element) -> Scene {
    let mut scene = Scene::new(name, "#ff00c0ff");
    for (uid, _) in cells {
        if scene.fixtures.iter().any(|(u, _)| u == uid) {
            continue;
        }
        if let Some(f) = patch.fixtures.iter().find(|f| &f.uid == uid) {
            scene.fixtures.push((uid.clone(), under_effect(patch, f)));
        }
    }
    scene.racks.push(rack);
    scene
}

/// The desk's own effects on this rig, which it runs live: their speed
/// and size follow its faders, where a cooked scene is fixed.
fn native_bank(patch: &Patch) -> Vec<Scene> {
    let of = |bank: &str| -> Vec<&Fixture> {
        let mut list: Vec<&Fixture> = patch
            .fixtures
            .iter()
            .filter(|f| bank_of(patch, f) == bank)
            .collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    };
    // The back-wall Solenas, then the front RockStrips, each left to
    // right: the rainbow rolls along the wall and then the front.
    let by_x = |prefix: &str| -> Vec<&Fixture> {
        let mut list: Vec<&Fixture> = of("BARS")
            .into_iter()
            .filter(|f| f.name.starts_with(prefix))
            .collect();
        list.sort_by_key(|f| f.x);
        list
    };
    let solena = by_x("Wall Bar");
    let strips = by_x("RockStrip");
    let cells_of = |fixtures: &[&Fixture], beams: std::ops::Range<u16>| -> Vec<Beam> {
        fixtures
            .iter()
            .flat_map(|f| beams.clone().map(|b| (f.uid.clone(), b)))
            .collect()
    };
    let bar_cells = |fixtures: &[&Fixture]| -> Vec<Beam> {
        fixtures
            .iter()
            .flat_map(|f| {
                let n = patch.profile(&f.profile).map_or(1, |p| cells(p).len().max(1));
                (0..u16::try_from(n).unwrap_or(1)).map(|b| (f.uid.clone(), b))
            })
            .collect()
    };
    // A tower's RGB surround is drawn on beams 6-11 (library::zq01334_33ch).
    let mut towers = of("TOWERS");
    towers.sort_by_key(|f| f.x);
    let tower_rgb = cells_of(&towers, 6..12);
    let pars = cells_of(&of("PARS"), 0..1);
    let mut bars = bar_cells(&solena);
    bars.extend(bar_cells(&strips));
    let movers = cells_of(&of("MOVERS"), 0..2);
    let mut everything = pars.clone();
    everything.extend(tower_rgb.clone());
    everything.extend(bars.clone());
    let mut perlin = pars;
    perlin.extend(tower_rgb.iter().cloned());
    vec![
        effect_scene("RAINBOW bars", patch, &bars, fx::color_fx(ColorEffect::Rainbow, &bars)),
        effect_scene(
            "KNIGHT RIDER towers",
            patch,
            &tower_rgb,
            fx::color_fx(ColorEffect::KnightRider, &tower_rgb),
        ),
        effect_scene(
            "SPARKLE all",
            patch,
            &everything,
            fx::color_fx(ColorEffect::Sparkle, &everything),
        ),
        effect_scene(
            "PERLIN pars towers",
            patch,
            &perlin,
            fx::color_fx(ColorEffect::Perlin, &perlin),
        ),
        effect_scene("MOVE SLOW", patch, &movers, fx::move_fx(MoveEffect::Slow, &movers)),
        effect_scene("MOVE FAST", patch, &movers, fx::move_fx(MoveEffect::Fast, &movers)),
    ]
}
