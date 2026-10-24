//! The device-code sign-in (RFC 8628) against the account server a Task
//! server trusts, runtime-agnostic over a [`Transport`] and a [`Clock`].
//!
//! [`start`] asks for a code; [`finish`] polls until it is approved,
//! refused or runs out. A network error or a 5xx while polling is **not** a
//! failure: the person may be approving on a phone while this laptop's
//! hotspot blinks. It is reported through `on_offline(true)` (the panel's
//! amber "Offline. Still waiting" line) and polling carries on until the
//! code's wall-clock [`Started::expires_at`].

use std::future::Future;
use std::time::{Duration, SystemTime};

use auth_client::StoredSession;
use auth_client::device::{self, Poll};

use crate::{Failure, Started};

/// An HTTP answer: its status and body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: String,
}

impl Answer {
    /// A 2xx.
    #[must_use]
    pub const fn ok(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}

/// The request could not be made or answered (no network, DNS, TLS, a
/// timeout): what the person sees as offline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unreachable(pub String);

/// How the sign-in talks to the servers.
pub trait Transport {
    /// GET `url`.
    fn get(&self, url: &str) -> impl Future<Output = Result<Answer, Unreachable>>;
    /// POST the JSON `body` to `url`.
    fn post_json(
        &self,
        url: &str,
        body: String,
    ) -> impl Future<Output = Result<Answer, Unreachable>>;
}

/// The wall clock and a way to wait.
pub trait Clock {
    fn now(&self) -> SystemTime;
    fn sleep(&self, wait: Duration) -> impl Future<Output = ()>;
}

/// The real network.
#[derive(Clone, Debug, Default)]
pub struct Http(reqwest::Client);

impl Http {
    #[must_use]
    pub fn new() -> Self {
        Self(
            reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        )
    }
}

impl Transport for Http {
    async fn get(&self, url: &str) -> Result<Answer, Unreachable> {
        answer(self.0.get(url).send().await).await
    }

    async fn post_json(&self, url: &str, body: String) -> Result<Answer, Unreachable> {
        answer(
            self.0
                .post(url)
                .header("content-type", "application/json")
                .body(body)
                .send()
                .await,
        )
        .await
    }
}

async fn answer(sent: reqwest::Result<reqwest::Response>) -> Result<Answer, Unreachable> {
    let response = sent.map_err(|e| Unreachable(e.to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| Unreachable(e.to_string()))?;
    Ok(Answer { status, body })
}

/// The system clock and tokio's timer.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    async fn sleep(&self, wait: Duration) {
        tokio::time::sleep(wait).await;
    }
}

/// Start signing in to `server` as `client_id`: read which account server
/// it trusts, and ask that for a code.
///
/// # Errors
///
/// [`Failure::Offline`] when either could not be reached,
/// [`Failure::NoAccountServer`] when the server names none, and the
/// server's words when it said something this cannot read.
pub async fn start(
    http: &impl Transport,
    clock: &impl Clock,
    server: &str,
    client_id: &str,
) -> Result<Started, Failure> {
    let well_known = http
        .get(&format!("{server}/.well-known/task-server.json"))
        .await
        .map_err(|_| Failure::Offline)?;
    if well_known.status >= 500 {
        return Err(Failure::Offline);
    }
    let issuer =
        device::issuer_from_well_known(&well_known.body).ok_or(Failure::NoAccountServer)?;
    let started = http
        .post_json(
            &format!("{issuer}{}", device::CODE_PATH),
            device::start_body(client_id),
        )
        .await
        .map_err(|_| Failure::Offline)?;
    if started.status >= 500 {
        return Err(Failure::Offline);
    }
    if !started.ok() {
        return Err(Failure::Other(started.body));
    }
    let code = device::parse_start(&started.body)?;
    tracing::info!(auth.flow = "device", auth.issuer = %issuer, "account: sign-in started");
    Ok(Started::new(issuer, code, clock.now()))
}

/// Wait for `started` to be approved, polling as the server asks.
/// `on_offline` is told whenever polling goes offline (`true`) or comes
/// back (`false`). The session is **not** kept here; the caller saves it
/// (and shows the save warning if that fails).
///
/// # Errors
///
/// [`Failure::Declined`], [`Failure::Expired`] (by the server, or the wall
/// clock past `expires_at` — after a suspend too), or the server's words.
pub async fn finish(
    http: &impl Transport,
    clock: &impl Clock,
    started: &Started,
    client_id: &str,
    mut on_offline: impl FnMut(bool),
) -> Result<StoredSession, Failure> {
    let mut wait = started.code.interval();
    let mut offline = false;
    let url = format!("{}{}", started.issuer, device::TOKEN_PATH);
    loop {
        clock.sleep(wait).await;
        if clock.now() >= started.expires_at {
            return Err(Failure::Expired);
        }
        let answer = match http
            .post_json(
                &url,
                device::poll_body(&started.code.device_code, client_id),
            )
            .await
        {
            Ok(a) if a.status < 500 => a,
            Ok(_) | Err(_) => {
                if !offline {
                    offline = true;
                    tracing::info!(
                        auth.flow = "device",
                        "account: polling offline; carrying on"
                    );
                    on_offline(true);
                }
                continue;
            }
        };
        if offline {
            offline = false;
            on_offline(false);
        }
        match device::parse_poll(answer.ok(), &answer.body)? {
            Poll::Approved(session) => {
                tracing::info!(
                    auth.flow = "device",
                    auth.outcome = "approved",
                    "account: signed in"
                );
                return Ok(session);
            }
            Poll::Pending => {}
            Poll::SlowDown => wait = wait.saturating_add(device::SLOW_DOWN_STEP),
        }
    }
}

/// One sign-in from start to a kept session, on the real network: what an
/// app with no UI of its own (or a test harness) calls.
///
/// # Errors
///
/// As [`start`] and [`finish`].
pub async fn sign_in(
    server: &str,
    client_id: &str,
    on_code: impl FnOnce(&Started),
) -> Result<(crate::Account, bool), Failure> {
    let http = Http::new();
    let started = start(&http, &SystemClock, server, client_id).await?;
    on_code(&started);
    let session = finish(&http, &SystemClock, &started, client_id, |_| {}).await?;
    let saved = keep(&session);
    Ok((
        crate::Account {
            server: server.to_owned(),
            session,
        },
        saved,
    ))
}

/// Keep `session` in the shared store; `false` (logged) when it could not
/// be: signed in for now, asked again next launch.
pub fn keep(session: &StoredSession) -> bool {
    match crate::Store::shared().map(|s| s.save(session)) {
        Some(Ok(())) => true,
        Some(Err(e)) => {
            tracing::warn!(error = %e, "account: the session could not be kept");
            false
        }
        None => {
            tracing::warn!("account: no home directory to keep the session in");
            false
        }
    }
}
