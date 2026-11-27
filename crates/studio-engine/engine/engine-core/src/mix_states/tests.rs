//! What a mix names, and what a sweep finds no mix names.

use std::collections::HashSet;

use super::{named_in, unnamed};

#[test]
fn a_mix_names_its_states_by_hash() {
    let a = "0123456789abcdef0123456789abcdef";
    let b = "fedcba9876543210fedcba9876543210";
    let text = format!(
        r#"{{"fx":[{{"state":"@{a}"}},{{"state":"@{b}"}},{{"state":"@{a}0"}},{{"state":"@short"}},{{"name":"x@y"}}]}}"#
    );
    let mut named = HashSet::new();
    named_in(&text, &mut named);
    assert_eq!(named, HashSet::from([a.to_owned(), b.to_owned()]));
}

#[test]
fn a_state_no_other_key_names_is_unnamed() {
    let a = "0123456789abcdef0123456789abcdef";
    let b = "fedcba9876543210fedcba9876543210";
    let keys = [
        format!("state:{a}"),
        format!("state:{b}"),
        "snapshot:A".to_owned(),
    ];
    let value = |key: &str| (key == "snapshot:A").then(|| format!(r#"{{"state":"@{a}"}}"#));
    assert_eq!(unnamed(&keys, value), vec![b.to_owned()]);
}

#[cfg(feature = "native")]
#[test]
fn inline_states_become_references_kept_once() {
    use std::collections::BTreeMap;

    use super::{SECTION, compact, hash, state_key};

    let state = "QUJD".repeat(100);
    let mix = format!(
        r#"{{"name":"A","fx":[{{"guid":"a","state":"{state}"}},{{"guid":"b","state":"{state}"}},{{"guid":"c","state":""}}]}}"#
    );
    let mut ext = BTreeMap::from([(
        SECTION.to_owned(),
        BTreeMap::from([
            ("snapshot:A".to_owned(), mix.clone()),
            ("snapshot:B".to_owned(), mix),
            ("head".to_owned(), "A".to_owned()),
        ]),
    )]);
    assert_eq!(compact(&mut ext), 4);
    let section = &ext[SECTION];
    let h = hash(&state);
    assert_eq!(section[&state_key(&h)], state);
    assert_eq!(
        section["snapshot:A"],
        format!(
            r#"{{"name":"A","fx":[{{"guid":"a","state":"@{h}"}},{{"guid":"b","state":"@{h}"}},{{"guid":"c","state":""}}]}}"#
        )
    );
    assert_eq!(section["head"], "A");
    assert_eq!(compact(&mut ext), 0, "once is enough");
}
