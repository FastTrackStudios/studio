//! Seam 1 on desktop for creating an organization (studio#697): any
//! signed-in account creates one from Studio, and is its owner, checked
//! only through Studio's MCP door.
//!
//! The real Studio, a first-time user's, signed in against the stand-in
//! Drive node and account server of `first_run` (its harness). The account
//! server takes `POST /auth/organization/create` as the real one does
//! since FastTrackStudios/auth#4: with the app's own access token as the
//! bearer, the account it signs in the owner, a slug some org has refused.
//!
//! Over MCP it checks:
//! - `STUDIO_ORG_CREATE {name}`: the org made at the account server, its
//!   slug from the name, the account its owner;
//! - Studio started again in it (going into another org restarts a
//!   desktop Studio), and `STUDIO_ORGS` listing it;
//! - `STUDIO_FIRST_RUN`: Home's organizations name it;
//! - `STUDIO_ORG_CREATE` with a slug already taken: refused, nothing made.
//!
//! It opens a window, so it needs a desktop session: `just test-org-create`
//! builds Studio and runs it; ignored otherwise.

#![cfg(unix)]

use std::time::Duration;

use serde_json::json;

use super::first_run::{Made, ORG, WHO, signed_in, until};

const NAME: &str = "Night Owls, Choir!";
const SLUG: &str = "night-owls-choir";
/// What Home calls it once Studio has started again: the issuer's org
/// list carries slugs only, so a name given here is the slug in words.
const SHOWN: &str = "Night Owls Choir";

#[test]
#[ignore = "opens Studio's window (a desktop session): `just test-org-create`"]
fn a_signed_in_account_creates_an_org_owns_it_and_studio_is_in_it() {
    let sandbox = tempfile::tempdir().expect("a sandbox");
    let (stand, mut studio, mcp) = signed_in(sandbox.path(), "org-create-test");
    let before = until(Duration::from_secs(30), "the app's pages", || {
        mcp.run("STUDIO_ORGS", &json!({})).ok()
    });
    assert_eq!(before["orgs"], json!([ORG]), "{before}");

    // Created at the account server, as this account, its owner.
    let made = mcp
        .run("STUDIO_ORG_CREATE", &json!({ "name": NAME }))
        .expect("STUDIO_ORG_CREATE");
    assert_eq!(made["org"], SLUG, "the slug is the name's: {made}");
    assert_eq!(made["name"], NAME, "{made}");
    assert_eq!(made["role"], "owner", "{made}");
    assert_eq!(
        stand.made(),
        vec![Made {
            slug: SLUG.into(),
            name: NAME.into(),
            owner: WHO.into(),
        }],
        "the account server made it, the account signed in its owner"
    );

    // Started again in it (as after signing in), and it listed.
    let mcp = studio.door_after_restart(&mcp, Duration::from_secs(120));
    let orgs = until(
        Duration::from_secs(30),
        "Studio to be in the new org",
        || {
            mcp.run("STUDIO_ORGS", &json!({}))
                .ok()
                .filter(|o| o["here"] == SLUG)
        },
    );
    let listed = orgs["orgs"].as_array().cloned().unwrap_or_default();
    assert!(listed.contains(&json!(SLUG)), "{orgs}");
    assert!(
        listed.contains(&json!(ORG)),
        "the account's other org stays: {orgs}"
    );

    // Home's organizations name it.
    let home = until(Duration::from_secs(60), "Home to list the new org", || {
        mcp.run("STUDIO_FIRST_RUN", &json!({})).ok().filter(|f| {
            f["orgs"]
                .as_array()
                .is_some_and(|o| o.iter().any(|o| o["org"] == SLUG))
        })
    });
    let shown = home["orgs"]
        .as_array()
        .and_then(|o| o.iter().find(|o| o["org"] == SLUG))
        .expect("the new org on Home");
    assert_eq!(shown["name"], SHOWN, "{home}");
    assert_eq!(home["here"], SLUG, "{home}");

    // A slug some org has already: refused, nothing made, Studio stays.
    let taken = mcp.run("STUDIO_ORG_CREATE", &json!({ "name": "Adult Jam" }));
    let why = taken.expect_err("a taken slug is refused");
    assert!(
        why.contains(&format!("already an organization called {ORG}")),
        "{why}"
    );
    assert_eq!(stand.made().len(), 1, "nothing more made");
    let after = mcp.run("STUDIO_ORGS", &json!({})).expect("STUDIO_ORGS");
    assert_eq!(after["here"], SLUG, "{after}");

    studio.stop();
    // Every create carried the app's own sign-in (the stand-in refuses
    // any other bearer with a 401, as auth does).
    let asked = stand.asked();
    assert_eq!(
        asked
            .iter()
            .filter(|a| a.starts_with("POST /auth/organization/create"))
            .count(),
        2,
        "{asked:?}"
    );
}
