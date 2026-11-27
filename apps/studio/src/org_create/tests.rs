//! An org made through the real issuer's routes (FastTrackStudios/auth's
//! organization service over HTTP, on an in-memory database), and the
//! draft a typed name gives.

use std::cell::RefCell;
use std::sync::Arc;

use auth::backend_db::{AuthSeaOrmStorage, Migrator};
use auth::{ArchitectAuth, CreateEmailPasswordUser, ListOrganizations};
use sea_orm_migration::MigratorTrait as _;

use super::*;

// ── the draft ───────────────────────────────────────────────────────────

#[test]
fn a_name_gives_its_slug() {
    assert_eq!(
        draft("  Days to Praise! ", None),
        Ok(Draft {
            name: "Days to Praise!".into(),
            slug: "days-to-praise".into(),
        })
    );
}

#[test]
fn a_slug_given_is_kept_in_slug_form() {
    assert_eq!(
        draft("Adult Jam", Some("Jam Night")).map(|d| d.slug),
        Ok("jam-night".into())
    );
    assert_eq!(
        draft("Adult Jam", Some("  ")).map(|d| d.slug),
        Ok("adult-jam".into())
    );
}

#[test]
fn no_name_or_no_letters_is_refused_before_asking_anyone() {
    assert_eq!(draft("   ", None), Err(Refused::NoName));
    assert_eq!(draft("!!!", None), Err(Refused::NoSlug));
}

// ── the issuer ──────────────────────────────────────────────────────────

/// Real HTTP to a real listener.
struct Http;

impl Fetch for Http {
    async fn send(&self, r: Request) -> Option<(u16, String)> {
        let client = reqwest::Client::new();
        let mut req = client.request(r.method.parse().ok()?, &r.url).body(r.body);
        for (k, v) in r.headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.ok()?;
        let status = resp.status().as_u16();
        Some((status, resp.text().await.ok()?))
    }
}

/// The issuer, its organization routes served on loopback: its URL and
/// the engine behind it (to sign people up and read back what it holds).
async fn issuer() -> (String, ArchitectAuth<AuthSeaOrmStorage>) {
    let db = sea_orm::Database::connect("sqlite::memory:")
        .await
        .expect("sqlite");
    Migrator::up(&db, None).await.expect("migrations");
    let auth = ArchitectAuth::builder()
        .secret("a-secret-at-least-32-bytes-long!!")
        .storage(AuthSeaOrmStorage::new(db))
        .build()
        .expect("auth");
    let svc = Arc::new(auth::transport::vox::AuthVoxService::new(auth.clone()));
    let router = auth_proto::organization_service_http::mount(svc, axum::Router::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move { axum::serve(listener, router).await });
    (url, auth)
}

/// A new account's session.
async fn account(auth: &ArchitectAuth<AuthSeaOrmStorage>, email: &str) -> String {
    auth.create_email_password_user(CreateEmailPasswordUser {
        email: email.into(),
        password: "correct horse battery staple".into(),
        name: Some("Ada".into()),
        username: None,
        image: None,
        metadata_json: None,
        ip_address: None,
        user_agent: None,
    })
    .await
    .expect("account")
    .token
}

#[tokio::test]
async fn any_account_creates_an_org_and_is_its_owner() {
    let (url, auth) = issuer().await;
    let session = account(&auth, "ada@example.com").await;
    let d = draft("Adult Jam", None).unwrap();

    let made = create(&Http, &url, &session, &d).await;
    assert_eq!(
        made,
        Ok(Created {
            org: "adult-jam".into(),
            name: "Adult Jam".into(),
            role: "owner".into(),
        })
    );

    // The issuer lists it among the account's orgs, the account its owner.
    let orgs = auth
        .list_organizations(ListOrganizations {
            session_token: session,
        })
        .await
        .expect("list");
    let mine: Vec<(String, String)> = orgs
        .into_iter()
        .map(|b| (b.organization.slug, b.membership.role))
        .collect();
    assert_eq!(mine, vec![("adult-jam".to_string(), "owner".to_string())]);
}

#[tokio::test]
async fn a_slug_another_org_has_is_refused_as_taken() {
    let (url, auth) = issuer().await;
    let first = account(&auth, "ada@example.com").await;
    let second = account(&auth, "grace@example.com").await;
    let d = draft("Adult Jam", None).unwrap();
    create(&Http, &url, &first, &d).await.expect("first");

    assert_eq!(
        create(&Http, &url, &second, &d).await,
        Err(Refused::Taken("adult-jam".into()))
    );
}

#[tokio::test]
async fn without_a_session_nothing_is_made() {
    let (url, auth) = issuer().await;
    let d = draft("Adult Jam", None).unwrap();

    let refused = create(&Http, &url, "not-a-session", &d).await;
    assert!(
        matches!(refused, Err(Refused::SignIn | Refused::Issuer(_))),
        "{refused:?}"
    );
    let anyone = account(&auth, "ada@example.com").await;
    let orgs = auth
        .list_organizations(ListOrganizations {
            session_token: anyone,
        })
        .await
        .expect("list");
    assert!(orgs.is_empty());
}

// ── the trade for a session ─────────────────────────────────────────────

/// Answers one request, and keeps it.
struct Once {
    answer: (u16, &'static str),
    seen: RefCell<Option<Request>>,
}

impl Fetch for Once {
    async fn send(&self, r: Request) -> Option<(u16, String)> {
        *self.seen.borrow_mut() = Some(r);
        Some((self.answer.0, self.answer.1.to_string()))
    }
}

#[tokio::test]
async fn an_access_token_is_traded_for_a_session_at_the_token_endpoint() {
    let fetch = Once {
        answer: (
            200,
            r#"{"access_token":"the-session","issued_token_type":"urn:fasttrackstudio:params:oauth:token-type:session","token_type":"Bearer"}"#,
        ),
        seen: RefCell::new(None),
    };
    let session = session_for(&fetch, "https://auth.example/", "drive", "a.b+c").await;
    assert_eq!(session, Ok("the-session".into()));

    let seen = fetch.seen.into_inner().expect("a request");
    assert_eq!(seen.method, "POST");
    assert_eq!(seen.url, "https://auth.example/oauth2/token");
    assert_eq!(
        seen.body,
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange\
         &client_id=drive&subject_token=a.b%2Bc\
         &subject_token_type=urn%3Aietf%3Aparams%3Aoauth%3Atoken-type%3Aaccess_token\
         &requested_token_type=urn%3Afasttrackstudio%3Aparams%3Aoauth%3Atoken-type%3Asession"
    );
}

#[tokio::test]
async fn a_refused_trade_asks_for_a_sign_in() {
    let fetch = Once {
        answer: (400, r#"{"error":"invalid_grant"}"#),
        seen: RefCell::new(None),
    };
    assert_eq!(
        session_for(&fetch, "https://auth.example", "drive", "old").await,
        Err(Refused::SignIn)
    );
}

#[tokio::test]
async fn an_app_the_issuer_does_not_let_trade_is_told_so_not_sent_to_sign_in() {
    let fetch = Once {
        answer: (403, r#"{"error":"access_denied"}"#),
        seen: RefCell::new(None),
    };
    assert_eq!(
        session_for(&fetch, "https://auth.example", "drive", "fresh").await,
        Err(Refused::NotAllowed)
    );
}
