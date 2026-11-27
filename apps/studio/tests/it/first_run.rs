//! Seam 1 on desktop for the first run (studio#699): a fresh install, with
//! no config, gets from signing in to an open setlist with no setup
//! screens, checked only through Studio's MCP door.
//!
//! The real Studio (`STUDIO_APP`, else this package's `Studio` binary) is
//! started as a first-time user's: an empty home folder (no Studio folder,
//! no Drive copies, no token, no MCP discovery file), its own instance
//! (`STUDIO_INSTANCE`), and a Drive node and account server of this test's
//! on loopback (`DRIVE_NODE`). The two stand in for Drive and auth at the
//! HTTP they speak: `/orgs`, signed out until a bearer the account server
//! issued comes with it, then one org for the account; `/oauth2/authorize`
//! and `/oauth2/token`; and the org's setlist file, found by `/search` and
//! read by `/fetch`. The system's browser is a script on `PATH` (`open`,
//! `xdg-open`) that writes down the page it was asked to open; the test is
//! then the person in it, following the account server back to Studio's
//! loopback port.
//!
//! Over MCP it checks:
//! - `STUDIO_ACCOUNT`: signed out, naming the account server;
//! - `STUDIO_SIGN_IN`: the account server opened in the browser, Studio
//!   back from it signed in and started again;
//! - after the restart, `STUDIO_ACCOUNT` signed in as the account, with
//!   its org, and the app on Home (`/`): no setup screen between;
//! - `STUDIO_FIRST_RUN`: `ready`, the org with its setlist and the route it
//!   opens at;
//! - `STUDIO_NAVIGATE` to it, then `STUDIO_ROUTE`: the setlist open, its
//!   songs read.
//!
//! It opens a window, so it needs a desktop session: `just test-first-run`
//! builds Studio and runs it; ignored otherwise.

#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// The account the account server signs in, and what the node lists for it.
pub(super) const WHO: &str = "ada@example.com";
pub(super) const ORG: &str = "adult-jam";
/// The account server's tokens begin so (one per account, [`Account::token`]);
/// the node takes nothing else.
const TOKEN: &str = "first-run-access-token";
const CODE: &str = "first-run-code";

/// The org's one setlist file, where its computer keeps it.
const LOCATION: &str = "loc-ada";
const SET_PATH: &str = "Setlists/Sunday.setlist";
const SET_NAME: &str = "Sunday";
const SONGS: [&str; 2] = ["Amazing Grace", "How Great Thou Art"];
const SET_TEXT: &str =
    "# org: adult-jam\n# Sunday morning\n= Amazing Grace\n= How Great Thou Art\n";

#[test]
#[ignore = "opens Studio's window (a desktop session): `just test-first-run`"]
fn a_fresh_install_signs_in_sees_its_orgs_and_opens_a_setlist() {
    let sandbox = tempfile::tempdir().expect("a sandbox");
    let (stand, mut studio, mcp) = signed_in(sandbox.path(), "first-run-test");
    let orgs = until(Duration::from_secs(30), "the app's pages", || {
        mcp.run("STUDIO_ORGS", &json!({})).ok()
    });
    assert_eq!(orgs["orgs"], json!([ORG]), "only the account's org: {orgs}");
    assert_eq!(orgs["here"], ORG, "{orgs}");
    let route = mcp.run("STUDIO_ROUTE", &json!({})).expect("STUDIO_ROUTE");
    assert_eq!(route["route"], "/", "a first run lands on Home: {route}");

    // Home offers the org's setlist, one tap from open.
    let first = until(Duration::from_secs(60), "Home to offer the setlist", || {
        mcp.run("STUDIO_FIRST_RUN", &json!({}))
            .ok()
            .filter(|f| f["step"] == "ready")
    });
    let org = &first["orgs"][0];
    assert_eq!(org["org"], ORG, "{first}");
    let offered = org["setlists"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["title"] == SET_NAME))
        .unwrap_or_else(|| panic!("no {SET_NAME} offered: {first}"));
    let to = offered["route"].as_str().expect("its route").to_string();

    // Opened: the page shows it, its songs read.
    mcp.run("STUDIO_NAVIGATE", &json!({ "path": to }))
        .expect("STUDIO_NAVIGATE");
    let shown = until(Duration::from_secs(60), "the setlist to open", || {
        mcp.run("STUDIO_ROUTE", &json!({})).ok().filter(|r| {
            r["shown"]["songs"]
                .as_array()
                .is_some_and(|s| !s.is_empty())
        })
    });
    assert_eq!(shown["route"], to, "{shown}");
    assert!(
        shown["shown"]["name"]
            .as_str()
            .is_some_and(|n| n.contains(SET_NAME)),
        "{shown}"
    );
    assert_eq!(shown["shown"]["songs"], json!(SONGS), "{shown}");

    studio.stop();
    let asked = stand.asked();
    assert!(
        asked.iter().any(|a| a.starts_with("POST /oauth2/token")),
        "the code was traded at the account server: {asked:?}"
    );
}

/// A first-time user's Studio, started on `root` against a stand-in node
/// and account server, signed in through the browser and started again:
/// checked on the way, and answering on its new door. `instance` is its
/// `STUDIO_INSTANCE` (its doors and engine its own), one per test.
pub(super) fn signed_in(root: &Path, instance: &'static str) -> (Stand, Studio, Mcp) {
    let stand = Stand::start();
    let (studio, mcp) = signed_in_as(&stand, ADA, root, instance);
    (stand, studio, mcp)
}

/// [`signed_in`] as `who`, against `stand` (one stand-in, any number of
/// Studios, each on its own `root` and `instance`).
pub(super) fn signed_in_as(
    stand: &Stand,
    who: Account,
    root: &Path,
    instance: &'static str,
) -> (Studio, Mcp) {
    stand.browser_signs_in(who);
    let opened = root.join("opened.txt");
    let bin = fake_browser(&root.join("bin"), &opened);
    let mut studio = Studio::start(root, instance, stand, &bin, &opened);

    // Signed out: the node wants a sign-in through its account server.
    let mcp = studio.door(Duration::from_secs(120));
    let account = mcp
        .run("STUDIO_ACCOUNT", &json!({}))
        .expect("STUDIO_ACCOUNT");
    assert_eq!(account["signed_in"], false, "{account}");
    assert_eq!(account["signed_out"], true, "{account}");
    assert_eq!(account["issuer"], stand.origin, "{account}");
    // Nothing of the app's own is up yet: it says so at once.
    let early = mcp.run("STUDIO_ORGS", &json!({}));
    assert!(early.is_err(), "STUDIO_ORGS before signing in: {early:?}");

    // Sign in: the account server in the browser, then back to Studio.
    let signing = {
        let mcp = mcp.clone();
        std::thread::spawn(move || mcp.run("STUDIO_SIGN_IN", &json!({})))
    };
    let page = until(Duration::from_secs(60), "the browser to be opened", || {
        std::fs::read_to_string(&opened).ok().and_then(|t| {
            t.lines()
                .find(|l| l.contains("/oauth2/authorize"))
                .map(str::to_string)
        })
    });
    assert!(page.starts_with(&stand.origin), "the browser opened {page}");
    browse(&page);
    let signed = signing
        .join()
        .expect("the sign-in's thread")
        .expect("STUDIO_SIGN_IN");
    assert_eq!(signed["signed_in"], true, "{signed}");
    assert_eq!(signed["restarting"], true, "{signed}");

    // Started again, signed in, on Home: no setup screen between.
    let mcp = studio.door_after_restart(&mcp, Duration::from_secs(120));
    let account = until(Duration::from_secs(30), "Studio to be signed in", || {
        mcp.run("STUDIO_ACCOUNT", &json!({}))
            .ok()
            .filter(|a| a["signed_in"] == true)
    });
    assert_eq!(account["who"], who.email, "{account}");
    assert_eq!(account["orgs"], json!(stand.orgs_of(who)), "{account}");
    (studio, mcp)
}

// ── the browser ─────────────────────────────────────────────────────────

/// A folder whose `open` and `xdg-open` write down what they were asked to
/// open, in `opened`: the system's browser, for Studio.
fn fake_browser(dir: &Path, opened: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).expect("the browser's folder");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\n",
        opened.display()
    );
    for name in ["open", "xdg-open"] {
        let p = dir.join(name);
        std::fs::write(&p, &script).expect("the browser");
        let mut perm = std::fs::metadata(&p).expect("its mode").permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
        std::fs::set_permissions(&p, perm).expect("made runnable");
    }
    dir.to_path_buf()
}

/// What a person's browser does with the account server's page: the
/// account server sends it back to Studio (a redirect), and it goes.
fn browse(page: &str) {
    let (status, head, _) = get(page);
    assert_eq!(status, 302, "the account server's page: {head}");
    let back = head
        .lines()
        .find_map(|l| {
            l.strip_prefix("Location: ")
                .or_else(|| l.strip_prefix("location: "))
        })
        .expect("where it sends the browser")
        .trim()
        .to_string();
    assert!(back.starts_with("http://127.0.0.1:"), "back to {back}");
    let (status, _, _) = get(&back);
    assert_eq!(status, 200, "Studio's loopback page");
}

/// A plain GET: status, head, body.
fn get(url: &str) -> (u16, String, Vec<u8>) {
    let rest = url.strip_prefix("http://").expect("an http URL");
    let (at, path) = rest
        .split_once('/')
        .map_or((rest, "/".to_string()), |(a, p)| (a, format!("/{p}")));
    let mut s = TcpStream::connect(at).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(30))).ok();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: {at}\r\nConnection: close\r\n\r\n"
    )
    .expect("send");
    let mut reply = Vec::new();
    s.read_to_end(&mut reply).expect("read");
    let split = reply
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or(reply.len());
    let head = String::from_utf8_lossy(&reply[..split]).into_owned();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    (
        status,
        head,
        reply.get(split + 4..).unwrap_or_default().to_vec(),
    )
}

// ── the stand-in node and account server ────────────────────────────────

/// An account at the account server: its id there, address and name.
#[derive(Clone, Copy, Debug)]
pub(super) struct Account {
    pub(super) id: &'static str,
    pub(super) email: &'static str,
    pub(super) name: &'static str,
}

/// The account `first_run` signs in (`WHO`), owner of `ORG`.
pub(super) const ADA: Account = Account {
    id: "acct-ada",
    email: WHO,
    name: "Ada",
};
/// A second account, with an org of its own (`BOB_ORG`).
pub(super) const BOB: Account = Account {
    id: "acct-bob",
    email: "bob@example.com",
    name: "Bob",
};
pub(super) const BOB_ORG: &str = "bobs-band";
const ACCOUNTS: [Account; 2] = [ADA, BOB];

impl Account {
    /// The access token the account server issues it; the node and the
    /// account server take no other (as since auth#4, a first-party app's
    /// access token, here Drive's).
    fn token(self) -> String {
        format!("{TOKEN}-{}", self.id)
    }

    fn by_token(bearer: Option<&str>) -> Option<Self> {
        let t = bearer?.strip_prefix("Bearer ")?;
        ACCOUNTS.into_iter().find(|a| a.token() == t)
    }

    fn by(f: impl Fn(&Self) -> bool) -> Option<Self> {
        ACCOUNTS.into_iter().find(f)
    }
}

/// Drive's node and the account server, as their HTTP goes, on loopback.
pub(super) struct Stand {
    /// `http://127.0.0.1:<port>`: the issuer, and the node's address.
    pub(super) origin: String,
    asked: Arc<Mutex<Vec<String>>>,
    world: Arc<Mutex<World>>,
}

/// An org the account server made (`POST /auth/organization/create`):
/// its slug, its name, and who made it (its owner).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Made {
    pub(super) slug: String,
    pub(super) name: String,
    pub(super) owner: String,
}

/// What the account server keeps: its orgs and their members, the
/// invitations and links made, and who the browser is signed in as.
struct World {
    /// The account the account server's page signs in (the person at the
    /// browser): what a code it hands out stands for.
    browser: Account,
    orgs: Vec<Org>,
    invitations: Vec<Invitation>,
    /// Invite links: their secret and the org's id.
    links: Vec<(String, String)>,
    made: Vec<Made>,
}

struct Org {
    id: String,
    slug: String,
    name: String,
    /// Account ids and their roles, in the order they came.
    members: Vec<(String, String)>,
}

/// An invitation sent to an address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Invitation {
    pub(super) id: String,
    pub(super) org: String,
    pub(super) email: String,
    secret: String,
    pub(super) accepted: bool,
}

impl World {
    fn new() -> Self {
        let org = |id: &str, slug: &str, name: &str, owner: Account| Org {
            id: id.into(),
            slug: slug.into(),
            name: name.into(),
            members: vec![(owner.id.into(), "owner".into())],
        };
        Self {
            browser: ADA,
            orgs: vec![
                org("org-adult-jam", ORG, "Adult Jam", ADA),
                org("org-bobs-band", BOB_ORG, "Bob's Band", BOB),
            ],
            invitations: Vec::new(),
            links: Vec::new(),
            made: Vec::new(),
        }
    }

    /// The account's orgs, and its role in each.
    fn of(&self, who: Account) -> Vec<(&Org, &str)> {
        self.orgs
            .iter()
            .filter_map(|o| {
                o.members
                    .iter()
                    .find(|(m, _)| m == who.id)
                    .map(|(_, r)| (o, r.as_str()))
            })
            .collect()
    }

    fn role(&self, who: Account, org: &str) -> Option<String> {
        self.of(who)
            .into_iter()
            .find(|(o, _)| o.id == org || o.slug == org)
            .map(|(_, r)| r.to_string())
    }

    fn org_mut(&mut self, id: &str) -> Option<&mut Org> {
        self.orgs.iter_mut().find(|o| o.id == id)
    }
}

impl Stand {
    pub(super) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let origin = format!("http://{}", listener.local_addr().expect("addr"));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let world = Arc::new(Mutex::new(World::new()));
        let (o, a, w) = (origin.clone(), asked.clone(), world.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (o, a, w) = (o.clone(), a.clone(), w.clone());
                std::thread::spawn(move || serve(stream, &o, &a, &w));
            }
        });
        Self {
            origin,
            asked,
            world,
        }
    }

    pub(super) fn asked(&self) -> Vec<String> {
        self.asked.lock().map(|a| a.clone()).unwrap_or_default()
    }

    fn with<T>(&self, f: impl FnOnce(&mut World) -> T) -> T {
        f(&mut self.world.lock().expect("the stand-in's world"))
    }

    /// Sign `who` in at the account server's page: the next sign-in
    /// through the browser is theirs.
    pub(super) fn browser_signs_in(&self, who: Account) {
        self.with(|w| w.browser = who);
    }

    /// The orgs the account server has made, in order.
    pub(super) fn made(&self) -> Vec<Made> {
        self.with(|w| w.made.clone())
    }

    /// The slugs of `who`'s orgs, as the node lists them.
    pub(super) fn orgs_of(&self, who: Account) -> Vec<String> {
        self.with(|w| w.of(who).iter().map(|(o, _)| o.slug.clone()).collect())
    }

    /// The members of `slug`: account ids and roles, in the order they came.
    pub(super) fn members(&self, slug: &str) -> Vec<(String, String)> {
        self.with(|w| {
            w.orgs
                .iter()
                .find(|o| o.slug == slug)
                .map(|o| o.members.clone())
                .unwrap_or_default()
        })
    }

    /// The invitations sent to an address, in order.
    pub(super) fn invitations(&self) -> Vec<Invitation> {
        self.with(|w| w.invitations.clone())
    }

    /// How many invite links have been made.
    pub(super) fn link_count(&self) -> usize {
        self.with(|w| w.links.len())
    }
}

/// One request: its line, its headers (lower-cased names) and body.
fn serve(stream: TcpStream, origin: &str, asked: &Mutex<Vec<String>>, world: &Mutex<World>) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_lowercase(), v.trim().to_string()));
        }
    }
    let length: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    let _ = reader.read_exact(&mut body);
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or("/"));
    let bearer = headers
        .iter()
        .find(|(k, _)| k == "authorization")
        .map(|(_, v)| v.as_str());
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if let Ok(mut a) = asked.lock() {
        // Who asked, by account (never the token).
        let by = Account::by_token(bearer).map_or("-", |a| a.id);
        a.push(format!("{method} {path} {by}"));
    }
    let reply = match world.lock() {
        Ok(mut w) => answer(method, path, query, bearer, &body, origin, &mut w),
        Err(_) => json_reply(500, &json!({})),
    };
    let mut stream = reader.into_inner();
    let _ = stream.write_all(&reply);
}

/// What the node or the account server answers.
fn answer(
    method: &str,
    path: &str,
    query: &str,
    bearer: Option<&str>,
    body: &[u8],
    origin: &str,
    w: &mut World,
) -> Vec<u8> {
    let auth = json!({ "issuer": origin, "client_id": "drive" });
    let who = Account::by_token(bearer);
    match (method, path, who) {
        // The account's orgs: those it is a member of, by slug.
        ("GET", "/orgs", Some(who)) => {
            let orgs: Vec<&str> = w.of(who).iter().map(|(o, _)| o.slug.as_str()).collect();
            json_reply(
                200,
                &json!({ "auth": auth, "signed_in": true, "who": who.email, "orgs": orgs, "default": "" }),
            )
        }
        ("GET", "/orgs", None) => json_reply(
            200,
            &json!({ "auth": auth, "signed_in": false, "orgs": [], "default": "" }),
        ),
        // The account server: the person is signed in there already (as
        // `browser`), and it sends them straight back with a code.
        ("GET", "/oauth2/authorize", _) => {
            let redirect = param(query, "redirect_uri").unwrap_or_default();
            let state = param(query, "state").unwrap_or_default();
            let code = format!("{CODE}-{}", w.browser.id);
            let to = format!("{redirect}?code={code}&state={}", encode(&state));
            format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .into_bytes()
        }
        // A code, or a refresh, traded for that account's tokens.
        ("POST", "/oauth2/token", _) => {
            let form = String::from_utf8_lossy(body);
            let of = |prefix: &str, field: &str| {
                let v = param(&form, field)?;
                Account::by(|a| v == format!("{prefix}-{}", a.id))
            };
            match of(CODE, "code").or_else(|| of("first-run-refresh", "refresh_token")) {
                Some(a) => json_reply(
                    200,
                    &json!({
                        "access_token": a.token(), "refresh_token": format!("first-run-refresh-{}", a.id),
                        "token_type": "Bearer", "expires_in": 3600,
                    }),
                ),
                None => json_reply(400, &json!({ "error": "invalid_grant" })),
            }
        }
        // The account server's organization calls, as since auth#4/#5:
        // the bearer is a first-party app's access token (here, the one it
        // issued) or a session; anything else is a 401.
        (_, p, None) if p.starts_with("/auth/organization/") => json_reply(
            401,
            &json!({ "code": "unauthorized", "message": "invalid session" }),
        ),
        (_, p, Some(who)) if p.starts_with("/auth/organization/") => {
            let v: Value = serde_json::from_slice(body).unwrap_or_default();
            let (status, v) = organization(&p["/auth/organization/".len()..], who, &v, w);
            json_reply(status, &v)
        }
        // The org's files are for its members only, as on Drive's node:
        // read with the account's sign-in or not at all.
        ("GET", _, None) if path == "/search" || path.starts_with("/fetch/") => {
            json_reply(401, &json!({ "error": "sign in" }))
        }
        // The org's index: its one setlist file, for its members.
        ("GET", "/search", Some(who))
            if param(query, "exts").is_some_and(|e| e.split(',').any(|x| x == "setlist")) =>
        {
            let space = param(query, "space").unwrap_or_default();
            let results = if space == ORG && w.role(who, ORG).is_some() {
                json!([{
                    "location": LOCATION, "location_name": "Ada's Mac", "device": "dev-ada",
                    "device_name": "Ada's Mac", "online": true, "path": SET_PATH,
                    "name": "Sunday.setlist", "dir": false, "size": SET_TEXT.len(),
                }])
            } else {
                json!([])
            };
            json_reply(200, &json!({ "results": results }))
        }
        ("GET", "/search", _) => json_reply(200, &json!({ "results": [] })),
        ("GET", p, Some(who))
            if p == format!("/fetch/{ORG}/{LOCATION}")
                && param(query, "path").as_deref() == Some(SET_PATH) =>
        {
            if w.role(who, ORG).is_none() {
                return json_reply(403, &json!({ "error": "not a member" }));
            }
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{SET_TEXT}",
                SET_TEXT.len()
            )
            .into_bytes()
        }
        _ => json_reply(404, &json!({})),
    }
}

/// `/auth/organization/<call>` for `who`, each with auth's own check
/// (`r[auth.jwt.first-party-access]`): inviting and removing need an owner
/// or an admin, the list a member, joining the invitation or link. The
/// statuses and words are auth's.
fn organization(call: &str, who: Account, v: &Value, w: &mut World) -> (u16, Value) {
    let forbidden = || {
        (
            403,
            json!({ "code": "forbidden", "message": "insufficient organization permissions" }),
        )
    };
    let bad_credentials = || {
        (
            401,
            json!({ "code": "unauthorized", "message": "invalid credentials" }),
        )
    };
    let manages = |w: &World, org: &str| {
        w.role(who, org)
            .is_some_and(|r| r.split(',').any(|r| ["owner", "admin"].contains(&r.trim())))
    };
    let already_in = || {
        (
            400,
            json!({ "code": "invalid_input", "message": "organization member already exists" }),
        )
    };
    let admit = |w: &mut World, org: &str| -> Result<(), (u16, Value)> {
        let o = w.org_mut(org).ok_or_else(bad_credentials)?;
        if o.members.iter().any(|(m, _)| m == who.id) {
            return Err(already_in());
        }
        o.members.push((who.id.into(), "member".into()));
        Ok(())
    };
    match call {
        "create" => create_org(who, v, w),
        "list" => (
            200,
            Value::Array(
                w.of(who)
                    .iter()
                    .map(|(o, r)| {
                        json!({
                            "organization": { "id": o.id, "slug": o.slug, "name": o.name },
                            "membership": { "role": r, "user_id": who.id },
                        })
                    })
                    .collect(),
            ),
        ),
        "members" => {
            let org = v["organization_id"].as_str().unwrap_or_default();
            if w.role(who, org).is_none() {
                return forbidden();
            }
            let Some(o) = w.orgs.iter().find(|o| o.id == org) else {
                return forbidden();
            };
            let people = o
                .members
                .iter()
                .filter_map(|(m, role)| {
                    let a = Account::by(|a| a.id == m)?;
                    Some(json!({
                        "member": { "role": role, "user_id": a.id },
                        "user": { "id": a.id, "name": a.name, "email": a.email },
                    }))
                })
                .collect();
            (200, Value::Array(people))
        }
        "invite-member" => {
            let i = &v["invite"];
            let org = i["organization_id"].as_str().unwrap_or_default();
            let Some(email) = i["email"].as_str() else {
                return (400, json!({ "code": "invalid_input", "message": "email" }));
            };
            if !manages(w, org) {
                return forbidden();
            }
            let n = w.invitations.len() + 1;
            let inv = Invitation {
                id: format!("inv-{n}"),
                org: org.into(),
                email: email.into(),
                secret: format!("invitation-secret-{n}"),
                accepted: false,
            };
            let answer = json!({
                "invitation": { "id": inv.id, "organization_id": org, "email": email, "status": "pending" },
                "token": inv.secret,
            });
            w.invitations.push(inv);
            (200, answer)
        }
        "invite-link" => {
            let org = v["link"]["organization_id"].as_str().unwrap_or_default();
            if !manages(w, org) {
                return forbidden();
            }
            let secret = format!("link-secret-{}", w.links.len() + 1);
            w.links.push((secret.clone(), org.into()));
            (
                200,
                json!({ "link": { "id": format!("link-{}", w.links.len()), "organization_id": org }, "token": secret }),
            )
        }
        // A made-up link is answered as a bad sign-in is, as auth does.
        "join" => {
            let secret = v["link_token"].as_str().unwrap_or_default();
            let Some(org) = w
                .links
                .iter()
                .find(|(s, _)| s == secret)
                .map(|(_, o)| o.clone())
            else {
                return bad_credentials();
            };
            match admit(w, &org) {
                Ok(()) => (200, json!({ "organization_id": org, "role": "member" })),
                Err(e) => e,
            }
        }
        "accept-invitation" => {
            let (id, secret) = (
                v["invitation_id"].as_str().unwrap_or_default(),
                v["invitation_token"].as_str().unwrap_or_default(),
            );
            let Some(at) = w
                .invitations
                .iter()
                .position(|i| i.id == id && i.secret == secret)
            else {
                return bad_credentials();
            };
            let org = w.invitations[at].org.clone();
            if w.role(who, &org).is_some() {
                return already_in();
            }
            if w.invitations[at].accepted {
                return (
                    400,
                    json!({ "code": "invalid_input", "message": "invitation is not pending" }),
                );
            }
            match admit(w, &org) {
                Ok(()) => {
                    w.invitations[at].accepted = true;
                    (200, Value::Null)
                }
                Err(e) => e,
            }
        }
        // Someone not in it is answered as bad credentials, as auth does;
        // the last owner stays.
        "remove-member" => {
            let (org, gone) = (
                v["organization_id"].as_str().unwrap_or_default(),
                v["user_id"].as_str().unwrap_or_default(),
            );
            if !manages(w, org) {
                return forbidden();
            }
            let Some(o) = w.org_mut(org) else {
                return forbidden();
            };
            let Some(at) = o.members.iter().position(|(m, _)| m == gone) else {
                return bad_credentials();
            };
            let owners = o.members.iter().filter(|(_, r)| r == "owner").count();
            if o.members[at].1 == "owner" && owners == 1 {
                return (
                    400,
                    json!({ "code": "invalid_input", "message": "cannot remove the last organization owner" }),
                );
            }
            o.members.remove(at);
            (200, json!({ "removed": gone }))
        }
        _ => (404, json!({})),
    }
}

/// `POST /auth/organization/create` for the signed-in account: the org
/// made, the account its owner, or the slug refused as taken.
fn create_org(who: Account, v: &Value, w: &mut World) -> (u16, Value) {
    let org = &v["organization"];
    let (Some(name), Some(slug)) = (org["name"].as_str(), org["slug"].as_str()) else {
        return (
            400,
            json!({ "code": "invalid_input", "message": "organization name and slug" }),
        );
    };
    if w.orgs.iter().any(|o| o.slug == slug) {
        return (
            400,
            json!({ "code": "invalid_input", "message": "organization slug already exists" }),
        );
    }
    let id = format!("org-{slug}");
    w.orgs.push(Org {
        id: id.clone(),
        slug: slug.into(),
        name: name.into(),
        members: vec![(who.id.into(), "owner".into())],
    });
    w.made.push(Made {
        slug: slug.to_string(),
        name: name.to_string(),
        owner: who.email.to_string(),
    });
    (
        200,
        json!({
            "organization": { "id": id, "name": name, "slug": slug },
            "membership": { "role": "owner", "user": who.email },
        }),
    )
}

fn json_reply(status: u16, v: &Value) -> Vec<u8> {
    let body = v.to_string();
    format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        match status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            500 => "Internal Server Error",
            _ => "Not Found",
        },
        body.len()
    )
    .into_bytes()
}

fn param(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == name).then(|| decode(v))
    })
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                if let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(x);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

// ── Studio ──────────────────────────────────────────────────────────────

/// The Studio under test, in a process group of its own (it starts itself
/// again on signing in, and the new one is in the same group).
pub(super) struct Studio {
    child: Child,
    instance: &'static str,
    home: PathBuf,
    config: PathBuf,
}

impl Studio {
    fn start(
        root: &Path,
        instance: &'static str,
        stand: &Stand,
        bin: &Path,
        opened: &Path,
    ) -> Self {
        let app = std::env::var_os("STUDIO_APP").map_or_else(
            || PathBuf::from(env!("CARGO_BIN_EXE_Studio")),
            PathBuf::from,
        );
        let home = root.join("home");
        let config = home.join(".config");
        let runtime = root.join("run");
        for d in [&home, &config, &runtime] {
            std::fs::create_dir_all(d).expect("a folder of the sandbox");
        }
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let node = stand.origin.replacen("http://", "ws://", 1) + "/sync";
        let mut cmd = Command::new(app);
        cmd.env("HOME", &home)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("STUDIO_INSTANCE", instance)
            .env("DRIVE_NODE", node)
            .env("PATH", path)
            .env("FIRST_RUN_OPENED", opened);
        // A first-time user's: nothing of the person running the test.
        for k in [
            "STUDIO_HOME",
            "STUDIO_DATA_DIR",
            "DRIVE_AGENT_DIR",
            "DRIVE_CLIENT_DIR",
            "FTS_MCP",
            "FTS_MCP_PORT",
            "FTS_MCP_PRIVATE",
            "FTS_DAW_ENGINE_DIR",
            "BROWSER",
        ] {
            cmd.env_remove(k);
        }
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let child = cmd.spawn().expect("Studio started");
        Self {
            child,
            instance,
            home,
            config,
        }
    }

    /// Where the discovery file of this instance's MCP door is.
    fn discovery(&self) -> Option<PathBuf> {
        let name = format!("studio-{}-mcp.json", self.instance);
        [
            self.config.join("fts").join(&name),
            self.home
                .join("Library/Application Support/fts")
                .join(&name),
        ]
        .into_iter()
        .find(|p| p.is_file())
    }

    fn read_door(&self) -> Option<Mcp> {
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(self.discovery()?).ok()?).ok()?;
        Mcp::new(v["url"].as_str()?, v["token"].as_str()).ok()
    }

    /// The door, once Studio answers on it.
    fn door(&mut self, wait: Duration) -> Mcp {
        until(wait, "Studio's MCP door", || {
            if let Ok(Some(status)) = self.child.try_wait() {
                panic!("Studio stopped before its door opened: {status}");
            }
            self.read_door().filter(Mcp::ping)
        })
    }

    /// The new Studio's door, once it has replaced `old`.
    pub(super) fn door_after_restart(&self, old: &Mcp, wait: Duration) -> Mcp {
        until(wait, "Studio to start again", || {
            self.read_door().filter(|m| m.url != old.url && m.ping())
        })
    }

    /// Every process of the group: the Studio started and the one it
    /// started again.
    pub(super) fn stop(&mut self) {
        let group = format!("-{}", self.child.id());
        let _ = Command::new("kill").args(["-TERM", "--", &group]).status();
        let _ = self.child.wait();
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        let group = format!("-{}", self.child.id());
        let _ = Command::new("kill").args(["-KILL", "--", &group]).status();
    }
}

/// `f` until it gives something, or fail after `wait`.
pub(super) fn until<T>(wait: Duration, what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(start.elapsed() < wait, "waited {wait:?} for {what}");
        std::thread::sleep(Duration::from_millis(250));
    }
}

// ── the door ────────────────────────────────────────────────────────────

/// Studio's MCP door, as an agent reaches it: JSON-RPC over HTTP on
/// loopback with the token from its discovery file, every action through
/// the `run` tool.
#[derive(Clone, Debug)]
pub(super) struct Mcp {
    url: String,
    at: String,
    path: String,
    token: Option<String>,
}

impl Mcp {
    fn new(url: &str, token: Option<&str>) -> Result<Self, String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("not an http URL: {url}"))?;
        let (at, path) = rest.split_once('/').unwrap_or((rest, "mcp"));
        Ok(Self {
            url: url.to_string(),
            at: at.to_owned(),
            path: format!("/{path}"),
            token: token.map(str::to_string),
        })
    }

    fn ping(&self) -> bool {
        self.rpc("ping", &json!({})).is_ok()
    }

    pub(super) fn run(&self, id: &str, args: &Value) -> Result<Value, String> {
        let result = self.rpc(
            "tools/call",
            &json!({"name": "run", "arguments": {"id": id, "args": args}}),
        )?;
        let text = result["content"][0]["text"].as_str().unwrap_or_default();
        if result["isError"].as_bool() == Some(true) {
            return Err(format!("{id}: {text}"));
        }
        if let Some(structured) = result.get("structuredContent") {
            return Ok(structured.clone());
        }
        Ok(serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_owned())))
    }

    fn rpc(&self, method: &str, params: &Value) -> Result<Value, String> {
        let body =
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).to_string();
        let mut stream = TcpStream::connect(&self.at).map_err(|e| format!("{}: {e}", self.at))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10 * 60)))
            .map_err(|e| e.to_string())?;
        let auth = self
            .token
            .as_ref()
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let head = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n",
            self.path,
            self.at,
            body.len()
        );
        stream
            .write_all(head.as_bytes())
            .and_then(|()| stream.write_all(body.as_bytes()))
            .map_err(|e| format!("{method}: {e}"))?;
        let mut reply = Vec::new();
        stream
            .read_to_end(&mut reply)
            .map_err(|e| format!("{method}: {e}"))?;
        let split = reply
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or_else(|| format!("{method}: no HTTP reply"))?;
        let status = String::from_utf8_lossy(&reply[..split]);
        if !status.starts_with("HTTP/1.1 200") {
            return Err(format!(
                "{method}: {}",
                status.lines().next().unwrap_or_default()
            ));
        }
        let answer: Value = serde_json::from_slice(&reply[split + 4..])
            .map_err(|e| format!("{method}: the reply is not JSON: {e}"))?;
        if let Some(error) = answer.get("error") {
            return Err(format!("{method}: {error}"));
        }
        Ok(answer["result"].clone())
    }
}
