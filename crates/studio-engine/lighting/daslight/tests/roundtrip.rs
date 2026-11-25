//! A real showfile survives being read and written.

use std::path::Path;

use ignition_daslight::Showfile;

#[expect(clippy::unwrap_used, reason = "the fixture file is part of the test")]
fn template() -> Showfile {
    Showfile::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mobile-lighting.dvc"))
        .unwrap()
}

#[test]
fn reads_the_mobile_rig() {
    let show = template();
    let patch = show.patch().unwrap();
    let mut at: Vec<(u16, usize, &str)> = patch
        .fixtures
        .iter()
        .map(|f| (f.address, patch.footprint(f), f.name.as_str()))
        .collect();
    at.sort_unstable();
    assert_eq!(
        at,
        [
            (1, 7, "u'king 36 led rgb 7 channel par"),
            (8, 7, "u'king 36 led rgb 7 channel par"),
            (15, 15, "zq01334"),
            (30, 15, "zq01334"),
            (92, 7, "rockville rockstrip 252"),
        ]
    );
    assert_eq!(show.scene_names().len(), 9);
}

#[test]
fn an_unchanged_patch_writes_back_the_same() {
    let mut show = template();
    let before = show.patch().unwrap();
    show.set_patch(&before).unwrap();
    let again = Showfile::parse(&show.to_text()).unwrap();
    let after = again.patch().unwrap();
    assert_eq!(before.fixtures, after.fixtures);
    assert_eq!(before.profiles, after.profiles);
    // The rest of the file is untouched.
    assert_eq!(again.scene_names(), template().scene_names());
}

#[test]
fn the_whole_file_round_trips() {
    let show = template();
    let again = Showfile::parse(&show.to_text()).unwrap();
    assert_eq!(show.root(), again.root());
}

/// A scene written by the crate is byte for byte what the desk stored for
/// the same values: the par's RED scene (dimmer, red full; green, blue 0).
#[test]
fn scene_data_matches_the_desk() {
    use ignition_daslight::container::unpack_raw;
    use ignition_daslight::scene::fixture_data;
    let show = template();
    let patch = show.patch().unwrap();
    let par = patch
        .profile_named("UKing/U'King 36 LED RGB 7 Channel Par.ssl2")
        .unwrap();
    let ours = fixture_data(
        par,
        &[Some(255), Some(255), Some(0), Some(0), None, None, None],
    )
    .unwrap();
    let desk = unpack_raw("eJxiYGf4D4RA8B8MgAxGGxCXYQ8c2sBJBgAAAAD//w==").unwrap();
    assert_eq!(ours.get(..16), desk.get(..16), "values");
    let red_par = show
        .root()
        .descendants("FIXTUREDATA")
        .into_iter()
        .filter_map(|fd| unpack_raw(fd.get("DATA")?).ok())
        .find(|raw| raw.get(..16) == ours.get(..16))
        .unwrap();
    assert_eq!(ours, red_par);
}
