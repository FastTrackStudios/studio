use super::MissingOnce;

#[test]
fn a_missing_mixer_is_reported_once_per_absence() {
    let missing = MissingOnce::default();
    // The rig looks every few seconds; only the first miss says so.
    assert!(missing.first_miss(false));
    assert!(!missing.first_miss(false));
    assert!(!missing.first_miss(false));
    // Found, then gone again: a new absence, reported again.
    assert!(!missing.first_miss(true));
    assert!(missing.first_miss(false));
    assert!(!missing.first_miss(false));
}
