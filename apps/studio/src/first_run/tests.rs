//! The first run's choices: the node a fresh install reaches, the orgs it
//! lists, the step it is at, and the setlists Home offers.

use serde_json::json;

use super::*;

// ── the node ────────────────────────────────────────────────────────────

#[test]
fn a_fresh_install_reaches_fasttracks_node() {
    assert_eq!(default_node(Named::default()), Some(DEFAULT_NODE));
    assert!(
        DEFAULT_NODE.starts_with("wss://"),
        "a node across the internet is reached over TLS"
    );
}

#[test]
fn anything_that_names_a_node_is_left_to_name_it() {
    for named in [
        Named {
            env: true,
            ..Named::default()
        },
        Named {
            agent: true,
            ..Named::default()
        },
        Named {
            local: true,
            ..Named::default()
        },
    ] {
        assert_eq!(default_node(named), None, "{named:?}");
    }
}

#[test]
fn the_agent_names_a_node_only_with_one_in_its_config() {
    assert!(agent_names_node(Some(
        r#"{"node":"wss://drive.example/sync"}"#
    )));
    assert!(!agent_names_node(Some(r#"{"node":"  "}"#)));
    assert!(!agent_names_node(Some(r#"{"device":"x"}"#)));
    assert!(!agent_names_node(Some("not json")));
    assert!(!agent_names_node(None));
}

// ── the account ─────────────────────────────────────────────────────────

const ISSUER: &str = "https://auth.example";

fn signed_out() -> Value {
    json!({"auth": {"issuer": ISSUER, "client_id": "drive"}, "default": "", "orgs": [], "signed_in": false})
}

#[test]
fn a_node_that_wants_a_sign_in_is_read_as_signed_out() {
    let a = Account::read(&signed_out());
    assert!(a.signed_out);
    assert!(!a.signed_in());
    assert_eq!(a.issuer.as_deref(), Some(ISSUER));
}

#[test]
fn an_account_lists_the_nodes_orgs_and_never_the_made_up_ones() {
    let v = json!({"auth": {"issuer": ISSUER, "client_id": "drive"}, "who": "ada@example.com",
        "orgs": ["adult-jam", "days-to-praise"], "signed_in": true});
    let a = Account::read(&v);
    assert!(a.signed_in());
    assert_eq!(a.who, "ada@example.com");
    let had = vec!["crossroads-church".to_string(), "cbu".to_string()];
    let listed = orgs_listed(&a, &had);
    assert_eq!(listed.len(), 2);
    assert!(listed.contains(&"adult-jam".to_string()));
    assert!(listed.contains(&"days-to-praise".to_string()));
}

#[test]
fn a_new_account_in_no_org_lists_none() {
    let v =
        json!({"auth": {"issuer": ISSUER, "client_id": "drive"}, "orgs": [], "signed_in": true});
    let a = Account::read(&v);
    let listed = orgs_listed(&a, &["crossroads-church".to_string()]);
    assert!(listed.is_empty(), "{listed:?}");
    assert_eq!(step(&listed, 0), Step::NoOrganization);
}

#[test]
fn a_node_that_lets_anyone_in_and_lists_nothing_keeps_what_studio_had() {
    let v = json!({"orgs": [], "default": "home"});
    let a = Account::read(&v);
    assert!(!a.signed_out && !a.signed_in());
    assert_eq!(
        orgs_listed(&a, &["home".to_string()]),
        vec!["home".to_string()]
    );
}

// ── the step and the setlists ───────────────────────────────────────────

#[test]
fn in_an_org_with_no_setlist_says_so_and_with_one_is_ready() {
    let orgs = vec!["adult-jam".to_string()];
    assert_eq!(step(&orgs, 0), Step::NoSetlist);
    assert_eq!(step(&orgs, 1), Step::Ready);
}

fn face(id: &str, title: &str) -> crate::setlist::Face {
    crate::setlist::Face {
        id: id.into(),
        title: title.into(),
        ..Default::default()
    }
}

fn file(space: &str, path: &str) -> Hit {
    Hit {
        space: space.into(),
        location: "loc-1".into(),
        path: path.into(),
        name: path.rsplit('/').next().unwrap_or(path).into(),
        ..Hit::default()
    }
}

#[test]
fn each_org_offers_its_own_sets_and_files_each_one_tap_from_open() {
    let orgs = vec!["adult-jam".to_string(), "days-to-praise".to_string()];
    let sets = vec![face("s1", "Sunday")];
    let files = vec![
        file("adult-jam", "Setlists/Friday.setlist"),
        file("days-to-praise", "Sets/Christmas Eve.setlist"),
        // Someone else's org: not offered.
        file("cbu", "Recital.setlist"),
    ];
    let listed = offers(&orgs, "adult-jam", &sets, &files);
    assert_eq!(listed.len(), 2);

    let jam = &listed[0];
    assert_eq!(jam.org, "adult-jam");
    let titles: Vec<&str> = jam.setlists.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, vec!["Sunday", "Friday"]);
    assert_eq!(
        jam.setlists[0].open,
        Route::OrgSet {
            owner: "adult-jam".into(),
            id: "s1".into()
        }
    );
    assert_eq!(
        jam.setlists[1].open,
        Route::DriveSet {
            space: "adult-jam".into(),
            location: "loc-1".into(),
            path: vec!["Setlists".into(), "Friday.setlist".into()],
        }
    );

    let dtp = &listed[1];
    assert_eq!(dtp.setlists.len(), 1);
    assert_eq!(dtp.setlists[0].title, "Christmas Eve");
}

#[test]
fn a_file_of_a_set_the_index_already_has_is_offered_once() {
    let orgs = vec!["adult-jam".to_string()];
    let sets = vec![face("s1", "Sunday")];
    let files = vec![file("adult-jam", "Setlists/Sunday.setlist")];
    let listed = offers(&orgs, "adult-jam", &sets, &files);
    assert_eq!(listed[0].setlists.len(), 1);
}

#[test]
fn home_is_ready_once_an_organization_has_a_setlist() {
    let orgs = vec!["adult-jam".to_string()];
    let (step, listed) = on_home(&orgs, "adult-jam", &[], &[]);
    assert_eq!(step, Step::NoSetlist);
    assert_eq!(listed[0].setlists.len(), 0);
    let (step, _) = on_home(&orgs, "adult-jam", &[face("s1", "Sunday")], &[]);
    assert_eq!(step, Step::Ready);
    let (step, listed) = on_home(&[], "home", &[face("s1", "Sunday")], &[]);
    assert_eq!(step, Step::NoOrganization);
    assert!(listed.is_empty());
}

#[test]
fn the_answer_names_each_setlists_route() {
    let orgs = vec!["adult-jam".to_string()];
    let files = vec![file("adult-jam", "Friday.setlist")];
    let listed = offers(&orgs, "adult-jam", &[], &files);
    let v = answer(Step::Ready, "adult-jam", &listed);
    assert_eq!(v["step"], "ready");
    assert_eq!(v["orgs"][0]["org"], "adult-jam");
    assert_eq!(v["orgs"][0]["name"], "Adult Jam");
    assert_eq!(v["orgs"][0]["setlists"][0]["title"], "Friday");
    assert_eq!(
        v["orgs"][0]["setlists"][0]["route"],
        Route::DriveSet {
            space: "adult-jam".into(),
            location: "loc-1".into(),
            path: vec!["Friday.setlist".into()],
        }
        .to_string()
    );
}
