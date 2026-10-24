//! When and where a gig is played, and whether it is on now.
//!
//! A gig's `gig.json` may carry a `schedule`: each slot of the day — the
//! soundcheck, the show — with its start (RFC 3339, so it carries the
//! venue's offset and means one instant wherever the device is), its end
//! and its place:
//!
//! ```json
//! "schedule": [
//!   {"label": "Soundcheck", "start": "2026-10-24T14:00:00-07:00",
//!    "end": "2026-10-24T17:00:00-07:00", "place": "Pasadena Civic Auditorium"},
//!   {"label": "Show", "start": "2026-10-24T18:00:00-07:00",
//!    "end": "2026-10-24T21:00:00-07:00", "place": "Pasadena Civic Auditorium"}
//! ]
//! ```
//!
//! A slot is **on** from [`LEAD`] before it starts until it ends; Studio's
//! home opens a gig that is on by itself (and the player can stay on the
//! home instead). Times are shown in the venue's own offset — "Show 6:00
//! pm" means 6 pm in Pasadena, read on a laptop in any time zone.

use chrono::{DateTime, Duration, FixedOffset, Utc};
use serde_json::Value;

/// How long before a slot starts it counts as on: time to get to the venue,
/// set up, and have the set open already.
pub const LEAD: Duration = Duration::hours(3);

/// How long a slot with no end lasts.
const DEFAULT_LENGTH: Duration = Duration::hours(3);

/// One slot of a gig's day.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    /// "Soundcheck", "Show".
    pub label: String,
    pub start: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
    pub place: String,
}

impl Slot {
    /// On now: from [`LEAD`] before its start until its end.
    #[must_use]
    pub fn is_on(&self, now: DateTime<Utc>) -> bool {
        self.start.with_timezone(&Utc) - LEAD <= now && now <= self.end.with_timezone(&Utc)
    }

    /// "Show · 6:00 pm" on its day, "Show · Sat 24 Oct, 6:00 pm" on another,
    /// in the venue's offset.
    #[must_use]
    pub fn when(&self, now: DateTime<Utc>) -> String {
        let at = self.start;
        let today = now.with_timezone(at.offset()).date_naive() == at.date_naive();
        let time = at.format("%-I:%M %P");
        if today {
            format!("{} · {time}", self.label)
        } else {
            format!("{} · {}, {time}", self.label, at.format("%a %-d %b"))
        }
    }

    /// Its key in the preference that remembers a slot the player stayed
    /// on the home for (`name@start`).
    #[must_use]
    pub fn key(&self, gig: &str) -> String {
        format!("{gig}@{}", self.start.to_rfc3339())
    }
}

/// A `gig.json`'s `schedule`, earliest first. A slot whose start does not
/// parse is left out (and said so); one whose end does not parse, or comes
/// before its start, lasts [`DEFAULT_LENGTH`].
#[must_use]
pub fn read(root: &Value) -> Vec<Slot> {
    let text = |v: &Value, k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    let mut slots: Vec<Slot> = root
        .get("schedule")
        .and_then(Value::as_array)
        .map(|slots| {
            slots
                .iter()
                .filter_map(|s| {
                    let start = DateTime::parse_from_rfc3339(&text(s, "start"))
                        .inspect_err(|e| tracing::warn!(gig.slot = %text(s, "label"), "gig: a slot's start does not parse: {e}"))
                        .ok()?;
                    let end = DateTime::parse_from_rfc3339(&text(s, "end"))
                        .ok()
                        .filter(|end| *end > start)
                        .unwrap_or(start + DEFAULT_LENGTH);
                    Some(Slot {
                        label: Some(text(s, "label"))
                            .filter(|l| !l.is_empty())
                            .unwrap_or_else(|| "Set".to_owned()),
                        start,
                        end,
                        place: text(s, "place"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    slots.sort_by_key(|s| s.start);
    slots
}

/// The slot of `slots` that is on at `now` — the one starting first, so a
/// soundcheck shows before the show it leads into — if any.
#[must_use]
pub fn on_now(slots: &[Slot], now: DateTime<Utc>) -> Option<&Slot> {
    slots.iter().find(|s| s.is_on(now))
}

/// The next slot that has not ended at `now`.
#[must_use]
pub fn next(slots: &[Slot], now: DateTime<Utc>) -> Option<&Slot> {
    slots.iter().find(|s| s.end.with_timezone(&Utc) >= now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caa() -> Vec<Slot> {
        read(&serde_json::json!({
            "schedule": [
                {"label": "Show", "start": "2026-10-24T18:00:00-07:00",
                 "end": "2026-10-24T21:00:00-07:00", "place": "Pasadena Civic Auditorium"},
                {"label": "Soundcheck", "start": "2026-10-24T14:00:00-07:00",
                 "place": "Pasadena Civic Auditorium"},
                {"label": "Broken", "start": "6 pm"}
            ]
        }))
    }

    fn at(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .expect("a time")
            .with_timezone(&Utc)
    }

    #[test]
    fn a_slot_is_on_from_its_lead_until_its_end() {
        let slots = caa();
        assert_eq!(slots.len(), 2, "the unparsable slot is left out");
        assert_eq!(slots[0].label, "Soundcheck", "earliest first");
        assert_eq!(
            on_now(&slots, at("2026-10-24T10:59:00-07:00")),
            None,
            "before the lead"
        );
        assert_eq!(
            on_now(&slots, at("2026-10-24T11:00:00-07:00")).map(|s| s.label.as_str()),
            Some("Soundcheck")
        );
        // The soundcheck has no end: three hours, so 17:00; the show's lead
        // started at 15:00 and it ends at 21:00.
        assert_eq!(
            on_now(&slots, at("2026-10-24T17:30:00-07:00")).map(|s| s.label.as_str()),
            Some("Show")
        );
        assert_eq!(
            on_now(&slots, at("2026-10-24T21:00:01-07:00")),
            None,
            "after the show"
        );
    }

    #[test]
    fn the_same_instant_holds_in_any_time_zone() {
        let slots = caa();
        // 11:30 pm in London is 3:30 pm in Pasadena: the soundcheck.
        let london = at("2026-10-24T23:30:00+01:00");
        assert_eq!(
            on_now(&slots, london).map(|s| s.label.as_str()),
            Some("Soundcheck")
        );
        assert_eq!(
            slots[1].when(london),
            "Show · 6:00 pm",
            "the venue's time, on its day"
        );
        assert_eq!(
            slots[1].when(at("2026-10-22T12:00:00-07:00")),
            "Show · Sat 24 Oct, 6:00 pm"
        );
    }
}
