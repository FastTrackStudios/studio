//! Seam 1 on desktop for an org's people (studio#770, after #698): an
//! owner invites, a second account joins with the link, both are listed,
//! and the owner removes the second, all checked only through the MCP
//! doors of two real Studios.
//!
//! Two first-time users' Studios (each its own sandbox and
//! `STUDIO_INSTANCE`), Ada's and Bob's, signed in against one stand-in
//! Drive node and account server of `first_run` (its harness). The
//! account server keeps its orgs' members, invitations and links, and
//! takes the member calls as the real one does since
//! FastTrackStudios/auth#5: the app's own access token as the bearer, each
//! call held to its own check (an owner or an admin invites and removes,
//! a member lists, the link or invitation joins).
//!
//! Over MCP it checks:
//! - `STUDIO_ORG_INVITE`: Ada, the owner, invites Bob by email, then makes
//!   a link anyone may follow;
//! - `STUDIO_ORG_JOIN`: Bob joins with the link, and his Studio goes into
//!   the org (starting again) and offers its setlist; the email
//!   invitation, pasted after, says he is in already;
//! - a member does not invite;
//! - `STUDIO_ORG_MEMBERS`: both accounts, as either sees it;
//! - `STUDIO_ORG_REMOVE`: Bob's access ends (his Studio is told he is not
//!   in it, the node no longer lists it), and Ada, the last owner, stays.
//!
//! It opens two windows, so it needs a desktop session: `just
//! test-org-invites` builds Studio and runs it; ignored otherwise.

#![cfg(unix)]

use std::time::Duration;

use serde_json::{Value, json};

use super::first_run::{ADA, Account, BOB, BOB_ORG, ORG, Stand, signed_in_as, until};

const SET_NAME: &str = "Sunday";

/// A member row as STUDIO_ORG_MEMBERS answers it.
fn row(a: Account, role: &str) -> Value {
    json!({ "account": a.id, "name": a.name, "email": a.email, "role": role })
}

#[test]
#[ignore = "opens two Studio windows (a desktop session): `just test-org-invites`"]
fn an_owner_invites_a_second_account_joins_by_link_and_the_owner_removes_it() {
    let sandbox = tempfile::tempdir().expect("a sandbox");
    let stand = Stand::start();
    let (mut ada_studio, ada) = signed_in_as(
        &stand,
        ADA,
        &sandbox.path().join("ada"),
        "org-invites-owner",
    );
    let here = until(Duration::from_secs(30), "Ada's app's pages", || {
        ada.run("STUDIO_ORGS", &json!({})).ok()
    });
    assert_eq!(here["here"], ORG, "{here}");

    // The owner, alone in it so far, and who may invite.
    let alone = ada
        .run("STUDIO_ORG_MEMBERS", &json!({}))
        .expect("STUDIO_ORG_MEMBERS");
    assert_eq!(alone["org"], ORG, "{alone}");
    assert_eq!(alone["role"], "owner", "{alone}");
    assert_eq!(alone["manage"], true, "{alone}");
    assert_eq!(alone["members"], json!([row(ADA, "owner")]), "{alone}");

    // Invited by email: an invitation at the account server, for him.
    let by_email = ada
        .run("STUDIO_ORG_INVITE", &json!({ "email": BOB.email }))
        .expect("STUDIO_ORG_INVITE by email");
    let email_link = by_email["link"].as_str().expect("its link").to_string();
    assert!(
        email_link.starts_with(&format!("{}/invite/", stand.origin)),
        "{by_email}"
    );
    assert_eq!(by_email["email"], BOB.email, "{by_email}");
    let sent = stand.invitations();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(
        (sent[0].email.as_str(), sent[0].accepted),
        (BOB.email, false),
        "{sent:?}"
    );

    // And by a link anyone who has it may follow.
    let by_link = ada
        .run("STUDIO_ORG_INVITE", &json!({}))
        .expect("STUDIO_ORG_INVITE by link");
    let link = by_link["link"].as_str().expect("its link").to_string();
    assert!(
        link.starts_with(&format!("{}/join?token=", stand.origin)),
        "{by_link}"
    );
    assert_eq!(by_link["email"], Value::Null, "{by_link}");
    assert_eq!(stand.links(), 1);

    // Bob, another account, in his own org; not in Ada's.
    let (mut bob_studio, bob) = signed_in_as(
        &stand,
        BOB,
        &sandbox.path().join("bob"),
        "org-invites-joiner",
    );
    let his = until(Duration::from_secs(30), "Bob's app's pages", || {
        bob.run("STUDIO_ORGS", &json!({})).ok()
    });
    assert_eq!(his["here"], BOB_ORG, "{his}");
    assert_eq!(his["orgs"], json!([BOB_ORG]), "{his}");
    let outside = bob.run("STUDIO_ORG_MEMBERS", &json!({ "org": ORG }));
    let why = outside.expect_err("not a member yet");
    assert!(why.contains(&format!("you are not in {ORG}")), "{why}");

    // He joins with the link: a member at the account server.
    let joined = bob
        .run("STUDIO_ORG_JOIN", &json!({ "link": link }))
        .expect("STUDIO_ORG_JOIN");
    assert_eq!(joined["org"], ORG, "{joined}");
    assert_eq!(joined["role"], "member", "{joined}");
    assert_eq!(
        stand.members(ORG),
        vec![
            (ADA.id.to_string(), "owner".to_string()),
            (BOB.id.to_string(), "member".to_string())
        ]
    );

    // His Studio goes into it (starting again, as going into an org does
    // on desktop), lists it, and offers its setlist.
    let bob = bob_studio.door_after_restart(&bob, Duration::from_secs(120));
    let his = until(Duration::from_secs(30), "Bob's Studio in the org", || {
        bob.run("STUDIO_ORGS", &json!({}))
            .ok()
            .filter(|o| o["here"] == ORG)
    });
    let listed = his["orgs"].as_array().cloned().unwrap_or_default();
    assert!(listed.contains(&json!(ORG)), "{his}");
    assert!(listed.contains(&json!(BOB_ORG)), "his own org stays: {his}");
    let home = until(
        Duration::from_secs(60),
        "Bob's Home to offer the setlist",
        || {
            bob.run("STUDIO_FIRST_RUN", &json!({})).ok().filter(|f| {
                f["orgs"].as_array().is_some_and(|o| {
                    o.iter().any(|o| {
                        o["org"] == ORG
                            && o["setlists"]
                                .as_array()
                                .is_some_and(|s| s.iter().any(|s| s["title"] == SET_NAME))
                    })
                })
            })
        },
    );
    assert_eq!(home["here"], ORG, "{home}");

    // The email invitation, pasted once he is in: in already.
    let again = bob.run("STUDIO_ORG_JOIN", &json!({ "link": email_link }));
    let why = again.expect_err("already in");
    assert!(why.contains("already in that organization"), "{why}");
    // A member does not invite.
    let not_his = bob.run("STUDIO_ORG_INVITE", &json!({}));
    let why = not_his.expect_err("a member invites no one");
    assert!(why.contains("only an owner or an admin"), "{why}");
    assert_eq!(stand.links(), 1, "no link made");

    // Both listed, by account, as either sees it.
    let both = json!([row(ADA, "owner"), row(BOB, "member")]);
    let seen = ada
        .run("STUDIO_ORG_MEMBERS", &json!({}))
        .expect("STUDIO_ORG_MEMBERS");
    assert_eq!(seen["members"], both, "{seen}");
    let seen = bob
        .run("STUDIO_ORG_MEMBERS", &json!({}))
        .expect("STUDIO_ORG_MEMBERS, a member's");
    assert_eq!(seen["org"], ORG, "{seen}");
    assert_eq!(seen["members"], both, "{seen}");
    assert_eq!(seen["role"], "member", "{seen}");
    assert_eq!(seen["manage"], false, "{seen}");

    // The owner removes him: his access to it ends.
    let removed = ada
        .run("STUDIO_ORG_REMOVE", &json!({ "account": BOB.id }))
        .expect("STUDIO_ORG_REMOVE");
    assert_eq!(removed["removed"], BOB.id, "{removed}");
    assert_eq!(
        stand.members(ORG),
        vec![(ADA.id.to_string(), "owner".to_string())]
    );
    let gone = bob.run("STUDIO_ORG_MEMBERS", &json!({ "org": ORG }));
    let why = gone.expect_err("removed");
    assert!(why.contains(&format!("you are not in {ORG}")), "{why}");
    let account = bob
        .run("STUDIO_ACCOUNT", &json!({}))
        .expect("STUDIO_ACCOUNT");
    assert_eq!(account["orgs"], json!([BOB_ORG]), "{account}");
    let twice = ada.run("STUDIO_ORG_REMOVE", &json!({ "account": BOB.id }));
    let why = twice.expect_err("not in it any more");
    assert!(why.contains("not in it any more"), "{why}");

    // The last owner stays.
    let last = ada.run("STUDIO_ORG_REMOVE", &json!({ "account": ADA.id }));
    let why = last.expect_err("the last owner stays");
    assert!(why.contains("keeps at least one owner"), "{why}");
    assert_eq!(
        stand.members(ORG),
        vec![(ADA.id.to_string(), "owner".to_string())]
    );

    bob_studio.stop();
    ada_studio.stop();
    // Every member call carried an account's own sign-in (the stand-in
    // refuses any other bearer with a 401, as auth does).
    let asked = stand.asked();
    let calls: Vec<&String> = asked
        .iter()
        .filter(|a| a.contains(" /auth/organization/"))
        .collect();
    assert!(!calls.is_empty(), "{asked:?}");
    assert!(calls.iter().all(|a| !a.ends_with(" -")), "{calls:?}");
    for (call, by) in [
        ("invite-member", ADA),
        ("invite-link", ADA),
        ("join", BOB),
        ("accept-invitation", BOB),
        ("remove-member", ADA),
    ] {
        let line = format!("POST /auth/organization/{call} {}", by.id);
        assert!(asked.contains(&line), "{line} in {asked:?}");
    }
}
