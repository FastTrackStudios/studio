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
    fn post_json(&self, url: &str, body: String) -> impl Future<Output = Result<Answer, Unreachable>>;
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
        .post_json(&format!("{issuer}{}", device::CODE_PATH), device::start_body(client_id))
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
            .post_json(&url, device::poll_body(&started.code.device_code, client_id))
            .await
        {
            Ok(a) if a.status < 500 => a,
            Ok(_) | Err(_) => {
                if !offline {
                    offline = true;
                    tracing::info!(auth.flow = "device", "account: polling offline; carrying on");
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
                tracing::info!(auth.flow = "device", auth.outcome = "approved", "account: signed in");
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

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    use super::*;

    /// Answers in order; `None` is a network error.
    struct Script(RefCell<VecDeque<Option<Answer>>>, Cell<usize>);

    impl Script {
        fn new(answers: impl IntoIterator<Item = Option<(u16, &'static str)>>) -> Self {
            Self(
                RefCell::new(
                    answers
                        .into_iter()
                        .map(|a| {
                            a.map(|(status, body)| Answer {
                                status,
                                body: body.to_owned(),
                            })
                        })
                        .collect(),
                ),
                Cell::new(0),
            )
        }

        fn next(&self) -> Result<Answer, Unreachable> {
            self.1.set(self.1.get().saturating_add(1));
            self.0
                .borrow_mut()
                .pop_front()
                .flatten()
                .ok_or_else(|| Unreachable("no route to host".to_owned()))
        }
    }

    impl Transport for Script {
        async fn get(&self, _url: &str) -> Result<Answer, Unreachable> {
            self.next()
        }
        async fn post_json(&self, _url: &str, _body: String) -> Result<Answer, Unreachable> {
            self.next()
        }
    }

    /// A clock that jumps by each sleep (plus `suspend` once, after the
    /// first, as if the iPad app had been in the background).
    struct Fake {
        now: Cell<SystemTime>,
        suspend: Cell<Duration>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                now: Cell::new(SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)),
                suspend: Cell::new(Duration::ZERO),
            }
        }
    }

    impl Clock for Fake {
        fn now(&self) -> SystemTime {
            self.now.get()
        }
        async fn sleep(&self, wait: Duration) {
            let jump = wait.saturating_add(self.suspend.replace(Duration::ZERO));
            self.now.set(self.now.get() + jump);
        }
    }

    const WELL_KNOWN: &str = r#"{"central_auth":"https://auth.example.test/"}"#;
    const CODE: &str = r#"{"device_code":"dev-secret","user_code":"QK72-M9X4","verification_uri":"/auth/device","verification_uri_complete":"/auth/device?code=QK72-M9X4","expires_in_seconds":600,"interval_seconds":5}"#;
    const PENDING: &str = r#"{"code":"verification_required","message":"not yet"}"#;
    const APPROVED: &str = r#"{"token":"tok-1","user":{"id":"u-1","email":"jules.marlowe@example.test"}}"#;
    const REFUSED: &str = r#"{"code":"permission_denied","message":"the person refused"}"#;

    async fn started(clock: &Fake) -> Result<Started, Failure> {
        let http = Script::new([Some((200, WELL_KNOWN)), Some((200, CODE))]);
        start(&http, clock, "https://task.example.test", "signal-app").await
    }

    #[tokio::test]
    async fn a_started_sign_in_shows_its_code_and_links() -> Result<(), Failure> {
        let clock = Fake::new();
        let s = started(&clock).await?;
        assert_eq!(s.issuer, "https://auth.example.test");
        assert_eq!(s.link(), "https://auth.example.test/auth/device?code=QK72-M9X4");
        assert_eq!(s.other_device(), "auth.example.test/auth/device");
        assert_eq!(s.groups(), ["QK72", "M9X4"]);
        assert_eq!(s.minutes(), 10);
        assert_eq!(s.left(clock.now()), Duration::from_secs(600));
        Ok(())
    }

    #[tokio::test]
    async fn an_approved_code_signs_in() -> Result<(), Failure> {
        let clock = Fake::new();
        let s = started(&clock).await?;
        let http = Script::new([Some((400, PENDING)), Some((200, APPROVED))]);
        let session = finish(&http, &clock, &s, "signal-app", |_| {}).await?;
        assert_eq!(session.token, "tok-1");
        assert_eq!(session.email.as_deref(), Some("jules.marlowe@example.test"));
        Ok(())
    }

    /// STA-361: a refused code ends the sign-in as Declined, at once.
    #[tokio::test]
    async fn a_refused_code_is_declined() -> Result<(), Failure> {
        let clock = Fake::new();
        let s = started(&clock).await?;
        let http = Script::new([Some((400, PENDING)), Some((403, REFUSED)), Some((200, APPROVED))]);
        let got = finish(&http, &clock, &s, "signal-app", |_| {}).await;
        assert_eq!(got, Err(Failure::Declined));
        assert_eq!(http.1.get(), 2, "no poll after the refusal");
        Ok(())
    }

    /// Design §8.1: a network error or a 5xx while polling is not a failure.
    #[tokio::test]
    async fn a_network_error_while_polling_does_not_fail_the_sign_in() -> Result<(), Failure> {
        let clock = Fake::new();
        let s = started(&clock).await?;
        let http = Script::new([
            Some((400, PENDING)),
            None,
            Some((502, "bad gateway")),
            None,
            Some((400, PENDING)),
            Some((200, APPROVED)),
        ]);
        let mut seen = Vec::new();
        let session = finish(&http, &clock, &s, "signal-app", |o| seen.push(o)).await?;
        assert_eq!(session.token, "tok-1");
        assert_eq!(seen, [true, false], "offline once, then back");
        Ok(())
    }

    /// Design §2: expiry is on the wall clock, so a suspend that outlasts
    /// the code ends it even though only one poll interval "passed".
    #[tokio::test]
    async fn expired_is_raised_at_expires_at_even_after_a_suspend() -> Result<(), Failure> {
        let clock = Fake::new();
        let s = started(&clock).await?;
        clock.suspend.set(Duration::from_secs(700));
        let http = Script::new([Some((400, PENDING)); 3]);
        let got = finish(&http, &clock, &s, "signal-app", |_| {}).await;
        assert_eq!(got, Err(Failure::Expired));
        assert_eq!(http.1.get(), 0, "no poll once the code has run out");
        Ok(())
    }

    #[tokio::test]
    async fn offline_at_start_is_offline_and_no_issuer_is_no_account_server() {
        let clock = Fake::new();
        let none = Script::new([None]);
        assert_eq!(
            start(&none, &clock, "https://task.example.test", "x").await,
            Err(Failure::Offline)
        );
        let nameless = Script::new([Some((200, "{}"))]);
        assert_eq!(
            start(&nameless, &clock, "https://task.example.test", "x").await,
            Err(Failure::NoAccountServer)
        );
    }
}
