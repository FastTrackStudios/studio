//! An org's people through the real issuer's routes (FastTrackStudios/
//! auth's organization service over HTTP, on an in-memory database), each
//! person signed in as Studio is: an OIDC access token of a first-party
//! client (`drive`), never a session.
//!
//! The flow #698 asks for: an owner makes an org, invites by link and by
//! email; two people with their own accounts join; the member list is the
//! issuer's; a member may not invite or remove; the owner removes one,
//! whose access then ends.

use std::cell::RefCell;
use std::sync::Arc;

use auth::backend_db::{AuthSeaOrmStorage, Migrator};
use auth::{
    ArchitectAuth, AuthorizeOidc, CreateEmailPasswordUser, ExchangeOidcToken, OidcClientConfig,
};
use base64::Engine as _;
use sea_orm_migration::MigratorTrait as _;
use sha2::Digest as _;

use super::*;
use crate::org_create;

// ── a pasted link ───────────────────────────────────────────────────────

#[test]
fn a_pasted_link_names_its_invitation() {
    assert_eq!(
        link_of(" https://auth.example/join?token=abc "),
        Some(Link::Join {
            token: "abc".into()
        })
    );
    assert_eq!(
        link_of("https://auth.example/invite/7f3c?token=xyz&x=1"),
        Some(Link::Invitation {
            id: "7f3c".into(),
            token: "xyz".into()
        })
    );
    assert_eq!(link_of("https://auth.example/join"), None);
    assert_eq!(link_of("https://auth.example/join?token="), None);
    assert_eq!(link_of("https://auth.example/orgs/1?token=abc"), None);
    assert_eq!(link_of("hello"), None);
}

#[test]
fn an_owner_or_admin_manages_a_member_does_not() {
    assert!(manages("owner"));
    assert!(manages("member, admin"));
    assert!(!manages("member"));
    assert!(has_role("admin,owner", "owner"));
    assert!(!has_role("co-owner", "owner"));
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

type Auth = ArchitectAuth<AuthSeaOrmStorage>;

fn client(id: &str, first_party: bool) -> OidcClientConfig {
    OidcClientConfig {
        client_id: id.into(),
        client_secret: None,
        name: id.into(),
        redirect_uris: vec![format!("https://{id}.example.com/callback")],
        scopes: vec!["openid".into(), "email".into()],
        public_client: true,
        skip_consent: true,
        first_party,
        rotate_refresh_tokens: false,
        disabled: false,
        issue_sessions: false,
    }
}

/// The issuer, its organization routes served on loopback, with Drive's
/// `drive` client first-party (as deployed) and a third party beside it.
async fn issuer() -> (String, Auth) {
    let db = sea_orm::Database::connect("sqlite::memory:")
        .await
        .expect("sqlite");
    Migrator::up(&db, None).await.expect("migrations");
    let auth = ArchitectAuth::builder()
        .secret("a-secret-at-least-32-bytes-long!!")
        .base_url("https://auth.example.com")
        .oidc_client(client("drive", true))
        .oidc_client(client("elsewhere", false))
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

/// Someone with an account, signed in to `client_id` by redirect: their
/// account id and that app's access token (no session).
async fn signed_in(auth: &Auth, email: &str, name: &str, client_id: &str) -> (String, String) {
    let bundle = auth
        .create_email_password_user(CreateEmailPasswordUser {
            email: email.into(),
            password: "correct horse battery staple".into(),
            name: Some(name.into()),
            username: None,
            image: None,
            metadata_json: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .expect("account");
    (
        bundle.user.id.to_string(),
        app_token(auth, &bundle.token, client_id).await,
    )
}

/// The access token `client_id` holds for the person `session` signs in.
async fn app_token(auth: &Auth, session: &str, client_id: &str) -> String {
    let verifier = "correct-horse-battery-staple-verifier";
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()));
    let redirect = format!("https://{client_id}.example.com/callback");
    let code = auth
        .authorize_oidc(AuthorizeOidc {
            session_token: session.into(),
            client_id: client_id.into(),
            redirect_uri: redirect.clone(),
            response_type: "code".into(),
            scope: Some("openid email".into()),
            state: None,
            nonce: None,
            code_challenge: Some(challenge),
            code_challenge_method: Some("S256".into()),
            prompt: None,
        })
        .await
        .expect("authorize")
        .code;
    auth.exchange_oidc_token(ExchangeOidcToken {
        grant_type: "authorization_code".into(),
        code: Some(code),
        redirect_uri: Some(redirect),
        client_id: client_id.into(),
        client_secret: None,
        code_verifier: Some(verifier.into()),
        refresh_token: None,
    })
    .await
    .expect("exchange")
    .access_token
}

fn ids(people: &[Member]) -> Vec<(String, String)> {
    let mut v: Vec<_> = people
        .iter()
        .map(|m| (m.account.clone(), m.role.clone()))
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn an_owner_invites_people_join_with_any_account_and_the_owner_removes_one() {
    let (url, auth) = issuer().await;
    let (owner_id, owner) = signed_in(&auth, "ada@example.com", "Ada", "drive").await;
    // Grace's address is not the one invited: any account accepts.
    let (grace_id, grace) = signed_in(&auth, "grace@other.example", "Grace", "drive").await;
    let (linus_id, linus) = signed_in(&auth, "linus@example.com", "Linus", "drive").await;

    // The owner makes the org with the app's token (#697) …
    let draft = org_create::draft("Days to Praise", None).unwrap();
    org_create::create(&Http, &url, &owner, &draft)
        .await
        .expect("create");
    let here = org(&Http, &url, &owner, "days-to-praise")
        .await
        .expect("the owner's org");
    assert_eq!(here.role, "owner");
    assert!(here.manages());

    // … invites by email and by link …
    let by_email = invite_email(&Http, &url, &owner, &here, " grace@example.com ")
        .await
        .expect("invite by email");
    assert_eq!(by_email.email.as_deref(), Some("grace@example.com"));
    assert!(
        by_email.link.starts_with(&format!("{url}/invite/")),
        "{}",
        by_email.link
    );
    let by_link = invite_link(&Http, &url, &owner, &here)
        .await
        .expect("invite by link");
    assert!(
        by_link.link.starts_with(&format!("{url}/join?token=")),
        "{}",
        by_link.link
    );

    // … and each joins with their own account, from the link pasted.
    let joined = join(&Http, &url, &grace, &link_of(&by_email.link).unwrap())
        .await
        .expect("Grace accepts");
    assert_eq!(
        (joined.slug.as_str(), joined.role.as_str()),
        ("days-to-praise", "member")
    );
    assert!(!joined.manages());
    let joined = join(&Http, &url, &linus, &link_of(&by_link.link).unwrap())
        .await
        .expect("Linus follows the link");
    assert_eq!(joined.slug, "days-to-praise");

    // The member list is the issuer's: accounts by id, whoever asks.
    let everyone = members(&Http, &url, &grace, &here).await.expect("members");
    let mut want = vec![
        (owner_id.clone(), "owner".to_string()),
        (grace_id.clone(), "member".to_string()),
        (linus_id.clone(), "member".to_string()),
    ];
    want.sort();
    assert_eq!(ids(&everyone), want);
    assert_eq!(everyone[0].label(), "Ada", "the owner is listed first");
    let grace_row = everyone.iter().find(|m| m.account == grace_id).unwrap();
    assert_eq!(grace_row.email.as_deref(), Some("grace@other.example"));

    // A member neither invites nor removes.
    assert_eq!(
        invite_link(&Http, &url, &grace, &here).await,
        Err(Refused::NotAllowed)
    );
    assert_eq!(
        invite_email(&Http, &url, &grace, &here, "x@example.com").await,
        Err(Refused::NotAllowed)
    );
    assert_eq!(
        remove(&Http, &url, &grace, &here, &linus_id).await,
        Err(Refused::NotAllowed)
    );

    // The owner removes Linus: his access to the org ends.
    remove(&Http, &url, &owner, &here, &linus_id)
        .await
        .expect("remove");
    assert_eq!(
        members(&Http, &url, &linus, &here).await,
        Err(Refused::NotAllowed)
    );
    assert_eq!(
        org(&Http, &url, &linus, "days-to-praise").await,
        Err(Refused::NotIn("days-to-praise".into()))
    );
    let left = members(&Http, &url, &owner, &here).await.expect("members");
    assert!(left.iter().all(|m| m.account != linus_id));

    // The last owner stays.
    assert_eq!(
        remove(&Http, &url, &owner, &here, &owner_id).await,
        Err(Refused::LastOwner)
    );
}

#[tokio::test]
async fn an_invitation_accepted_or_made_up_does_not_admit_anyone() {
    let (url, auth) = issuer().await;
    let (_, owner) = signed_in(&auth, "ada@example.com", "Ada", "drive").await;
    let (_, grace) = signed_in(&auth, "grace@example.com", "Grace", "drive").await;
    let (_, linus) = signed_in(&auth, "linus@example.com", "Linus", "drive").await;
    let draft = org_create::draft("Adult Jam", None).unwrap();
    org_create::create(&Http, &url, &owner, &draft)
        .await
        .unwrap();
    let here = org(&Http, &url, &owner, "adult-jam").await.unwrap();

    let sent = invite_email(&Http, &url, &owner, &here, "grace@example.com")
        .await
        .unwrap();
    let link = link_of(&sent.link).unwrap();
    join(&Http, &url, &grace, &link).await.expect("accept");
    assert_eq!(
        join(&Http, &url, &linus, &link).await,
        Err(Refused::LinkGone),
        "an accepted invitation let a second person in"
    );
    // Accepting again, once in, says so rather than that it is gone.
    let again = invite_email(&Http, &url, &owner, &here, "grace@example.com")
        .await
        .unwrap();
    assert_eq!(
        join(&Http, &url, &grace, &link_of(&again.link).unwrap()).await,
        Err(Refused::AlreadyIn)
    );
    // The issuer answers a made-up link as it answers a bad sign-in (the
    // app tells them apart by refreshing first).
    let made_up = Link::Join {
        token: "made-up".into(),
    };
    assert_eq!(
        join(&Http, &url, &linus, &made_up).await,
        Err(Refused::SignIn)
    );
    assert_eq!(
        org(&Http, &url, &linus, "adult-jam").await,
        Err(Refused::NotIn("adult-jam".into()))
    );
}

#[tokio::test]
async fn a_third_partys_token_reads_and_changes_no_ones_org() {
    let (url, auth) = issuer().await;
    let bundle = auth
        .create_email_password_user(CreateEmailPasswordUser {
            email: "ada@example.com".into(),
            password: "correct horse battery staple".into(),
            name: None,
            username: None,
            image: None,
            metadata_json: None,
            ip_address: None,
            user_agent: None,
        })
        .await
        .unwrap();
    let owner = app_token(&auth, &bundle.token, "drive").await;
    let elsewhere = app_token(&auth, &bundle.token, "elsewhere").await;
    let draft = org_create::draft("Adult Jam", None).unwrap();
    org_create::create(&Http, &url, &owner, &draft)
        .await
        .unwrap();
    let here = org(&Http, &url, &owner, "adult-jam").await.unwrap();

    assert_eq!(
        members(&Http, &url, &elsewhere, &here).await,
        Err(Refused::SignIn)
    );
    assert_eq!(
        invite_link(&Http, &url, &elsewhere, &here).await,
        Err(Refused::SignIn)
    );
    assert_eq!(
        remove(&Http, &url, &elsewhere, &here, &bundle.user.id.to_string()).await,
        Err(Refused::SignIn)
    );
}

// ── what is sent ────────────────────────────────────────────────────────

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
async fn removing_sends_the_apps_token_and_the_issuers_ids() {
    let fetch = Once {
        answer: (200, "null"),
        seen: RefCell::new(None),
    };
    let here = Org {
        id: "org-uuid".into(),
        slug: "adult-jam".into(),
        name: "Adult Jam".into(),
        role: "owner".into(),
    };
    remove(&fetch, "https://auth.example/", "a.b.c", &here, "user-uuid")
        .await
        .expect("removed");
    let seen = fetch.seen.into_inner().expect("a request");
    assert_eq!(
        seen.url,
        "https://auth.example/auth/organization/remove-member"
    );
    assert!(
        seen.headers
            .iter()
            .any(|(k, v)| *k == "authorization" && v == "Bearer a.b.c")
    );
    let body: serde_json::Value = serde_json::from_str(&seen.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({ "organization_id": "org-uuid", "user_id": "user-uuid" })
    );
}
