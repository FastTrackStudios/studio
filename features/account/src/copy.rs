//! What every sign-in state says, for both apps and both platforms
//! (design spec STA-379 §3.2, §4.5): one place, so Signal and Session can't
//! drift.

use crate::{App, Failure};

/// Where the app runs: a desktop shares its session with the other app; an
/// iPad's sandbox does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Desktop,
    Ipad,
}

impl Platform {
    /// This build's platform.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "ios") {
            Self::Ipad
        } else {
            Self::Desktop
        }
    }

    /// "this computer" / "this iPad".
    #[must_use]
    pub const fn this_device(self) -> &'static str {
        match self {
            Self::Desktop => "this computer",
            Self::Ipad => "this iPad",
        }
    }
}

/// A sheet's title, body and buttons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Words {
    pub title: &'static str,
    pub body: String,
    /// The one primary button, if any.
    pub primary: Option<&'static str>,
    /// The quiet button (Not now / Close / Cancel), if any.
    pub secondary: Option<&'static str>,
}

/// Signed out (`Out`).
#[must_use]
pub fn out(app: App) -> Words {
    Words {
        title: "Sign in to FastTrackStudio",
        body: format!(
            "One account for all your devices. Sign in to bring your sets and rigs along \
             and let your devices find each other. {} works without an account.",
            app.name()
        ),
        primary: Some("Sign in"),
        secondary: Some("Not now"),
    }
}

/// Asking for a code (`Starting`).
#[must_use]
pub fn starting() -> Words {
    Words {
        title: "Getting a sign-in code",
        body: "Contacting FastTrackStudio…".to_owned(),
        primary: None,
        secondary: Some("Cancel"),
    }
}

/// Code shown: the title; the panel draws the rest.
pub const CODE_TITLE: &str = "Approve this sign-in";
pub const STEP_OPEN: &str = "Open the sign-in page and sign in if it asks.";
pub const STEP_CHECK: &str = "Check the page shows this code, then approve.";
pub const OPEN_PAGE: &str = "Open sign-in page";
pub const COPY_CODE: &str = "Copy code";
pub const COPY_LINK: &str = "Copy link";
pub const WAITING: &str = "Waiting for approval…";
pub const OFFLINE_WAITING: &str =
    "Offline. Still waiting; this carries on when you're back online.";
pub const ONLY_YOURS: &str = "Only approve a code you just asked this app for.";

/// "On another device? Go to {where} and enter it."
#[must_use]
pub fn other_device(at: &str) -> String {
    format!("On another device? Go to {at} and enter it.")
}

/// "Expires in 9 min" (whole minutes rounded up), then seconds under a
/// minute.
#[must_use]
pub fn expires_in(left_secs: u64) -> String {
    if left_secs < 60 {
        format!("Expires in {left_secs} s")
    } else {
        format!("Expires in {} min", left_secs.div_ceil(60))
    }
}

/// Approved, being kept (`Finishing`).
#[must_use]
pub fn finishing() -> Words {
    Words {
        title: "You're approved",
        body: "Finishing sign-in…".to_owned(),
        primary: None,
        secondary: None,
    }
}

/// Just signed in (`In`).
#[must_use]
pub fn signed_in_now(app: App, platform: Platform, email: &str) -> Words {
    let who = if email.is_empty() {
        "Signed in.".to_owned()
    } else {
        format!("Signed in as {email}.")
    };
    Words {
        title: "You're signed in",
        body: match platform {
            Platform::Desktop => format!(
                "{who} {} and {} on this computer now share it.",
                app.name(),
                app.other().name()
            ),
            Platform::Ipad => who,
        },
        primary: Some("Done"),
        secondary: None,
    }
}

/// Signed in by the other app while this one's sheet was open.
#[must_use]
pub fn signed_in_elsewhere(app: App) -> String {
    format!("Signed in with {}.", app.other().name())
}

/// The session was kept nowhere (`store().save` failed).
pub const SAVE_WARNING: &str =
    "Signed in, but this device couldn't save it. You'll be asked to sign in again next time.";

/// A failure's sheet.
#[must_use]
pub fn failed(app: App, failure: &Failure, server: &str, minutes: u64) -> Words {
    match failure {
        Failure::Offline => Words {
            title: "Can't reach FastTrackStudio",
            body: format!(
                "Check your internet connection and try again. {} keeps working without an account.",
                app.name()
            ),
            primary: Some("Try again"),
            secondary: Some("Close"),
        },
        Failure::NoAccountServer => Words {
            title: "This server has no sign-in",
            body: format!(
                "{server} didn't name an account server. If you changed FTS_TASK_SERVER, check it."
            ),
            primary: None,
            secondary: Some("Close"),
        },
        Failure::Expired => Words {
            title: "That code ran out",
            body: format!(
                "Sign-in codes last {minutes} minutes. Get a new one and approve it a little sooner."
            ),
            primary: Some("Get a new code"),
            secondary: Some("Close"),
        },
        Failure::Declined => Words {
            title: "Sign-in was declined",
            body: "The request was refused in the browser. If that wasn't you, you can ignore it; \
                   nothing was shared."
                .to_owned(),
            primary: Some("Try again"),
            secondary: Some("Close"),
        },
        Failure::Unreadable(_) | Failure::Other(_) => Words {
            title: "Sign-in didn't work",
            body: "FastTrackStudio answered in a way this app didn't understand. Try again. If it \
                   keeps happening, send us the details."
                .to_owned(),
            primary: Some("Try again"),
            secondary: Some("Close"),
        },
    }
}

/// The server refused a kept session (`Ended`).
#[must_use]
pub fn ended() -> Words {
    Words {
        title: "Your sign-in ended",
        body:
            "It expired, or you signed out on another device. Sign in again to keep your devices \
               connected. Your sets and rigs are still here."
                .to_owned(),
        primary: Some("Sign in again"),
        secondary: Some("Not now"),
    }
}

/// The sign-out confirm.
pub const SIGN_OUT_TITLE: &str = "Sign out of FastTrackStudio?";

/// The sign-out confirm's body: a desktop names both apps, an iPad this one.
#[must_use]
pub fn sign_out_body(app: App, platform: Platform) -> String {
    match platform {
        Platform::Desktop => format!(
            "This signs out {} and {} on this computer. Your sets, rigs and files stay here.",
            App::Session.name(),
            App::Signal.name()
        ),
        Platform::Ipad => format!(
            "This signs out {} on this iPad. Your sets, rigs and files stay here.",
            app.name()
        ),
    }
}

/// The signed-in sheet's caption: "Signed in on this computer" and, on a
/// desktop, that both apps share it.
#[must_use]
pub fn signed_in_here(platform: Platform) -> (String, Option<&'static str>) {
    (
        format!("Signed in on {}", platform.this_device()),
        match platform {
            Platform::Desktop => Some("Session and Signal share this sign-in."),
            Platform::Ipad => None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_counts_minutes_then_seconds() {
        assert_eq!(expires_in(540), "Expires in 9 min");
        assert_eq!(expires_in(61), "Expires in 2 min");
        assert_eq!(expires_in(40), "Expires in 40 s");
    }

    #[test]
    fn sign_out_names_both_apps_on_a_desktop_and_one_on_an_ipad() {
        assert!(sign_out_body(App::Signal, Platform::Desktop).contains("Session and Signal"));
        let ipad = sign_out_body(App::Signal, Platform::Ipad);
        assert!(ipad.contains("Signal on this iPad") && !ipad.contains("Session"));
    }

    #[test]
    fn expired_copy_uses_the_servers_minutes() {
        assert!(
            failed(App::Session, &Failure::Expired, "", 15)
                .body
                .contains("15 minutes")
        );
    }
}
