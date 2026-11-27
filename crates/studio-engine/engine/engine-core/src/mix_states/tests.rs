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
