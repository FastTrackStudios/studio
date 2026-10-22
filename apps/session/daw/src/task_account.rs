//! Signing this window in to Task, for its library: the shared
//! [`fts_account`] crate (device-code sign-in, the one kept session every
//! FTS app on this computer shares) plus what only Session needs — an
//! account's org libraries and which orgs its person is in.
//!
//! Blocking, on the engine's runtime, like everything a window's start
//! screen runs on a thread of its own.

use fts_account::flow::{self, Http, SystemClock};
pub use fts_account::{Account, App, Failure, SERVER, Started, server, sign_out, signed_in};

/// This app, to the account server.
pub const APP: App = App::Session;

/// An account's org libraries.
pub trait AccountLibrary {
    /// The org `org`'s library, as this person.
    fn library(&self, org: &str) -> session::library::Library;
}

impl AccountLibrary for Account {
    fn library(&self, org: &str) -> session::library::Library {
        session::library::Library {
            server: self
                .server
                .replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1),
            org: org.to_owned(),
            token: Some(self.session.token.clone()),
        }
    }
}

/// Start signing in to `server`: ask its account server for a code.
///
/// # Errors
///
/// The server or its account server could not be reached, or said
/// something this does not understand.
pub fn start(server: &str) -> eyre::Result<Started> {
    let runtime = crate::open::engine_runtime()?;
    Ok(runtime.block_on(flow::start(
        &Http::new(),
        &SystemClock,
        server,
        APP.client_id(),
    ))?)
}

/// Wait for `started` to be approved (or refused, or to run out), polling
/// as the server asks and carrying on through network errors; the session
/// is kept (in the shared file) once it is.
///
/// # Errors
///
/// The sign-in was refused or ran out, or the account server stopped
/// answering sensibly.
pub fn finish(server: &str, started: &Started) -> eyre::Result<Account> {
    let runtime = crate::open::engine_runtime()?;
    let session = runtime.block_on(flow::finish(
        &Http::new(),
        &SystemClock,
        started,
        APP.client_id(),
        |_| {},
    ))?;
    // Not fatal when it could not be kept: signed in for now, asked again
    // next launch (logged by `keep`).
    let _ = flow::keep(&session);
    Ok(Account {
        server: server.to_owned(),
        session,
    })
}

/// The orgs on the account's server this person is in (every org the
/// server hosts, when it does not say).
///
/// # Errors
///
/// The server could not be reached, or its answer read.
pub fn orgs(account: &Account) -> eyre::Result<Vec<String>> {
    let runtime = crate::open::engine_runtime()?;
    let doc: serde_json::Value = runtime.block_on(async {
        Ok::<_, eyre::Report>(
            reqwest::Client::new()
                .get(format!("{}/.well-known/task-server.json", account.server))
                .bearer_auth(&account.session.token)
                .send()
                .await?
                .json()
                .await?,
        )
    })?;
    // (slug, whether this person is in it — `None` when the server does
    // not say).
    let orgs: Vec<(String, Option<bool>)> = doc
        .get("orgs")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|org| {
            let slug = org.get("slug")?.as_str()?.to_owned();
            Some((slug, org.get("member").and_then(serde_json::Value::as_bool)))
        })
        .collect();
    let anyone = orgs.iter().all(|(_, member)| member.is_none());
    Ok(orgs
        .into_iter()
        .filter(|(_, member)| anyone || *member == Some(true))
        .map(|(slug, _)| slug)
        .collect())
}
