//! What the launcher's pages show, as one view model
//! (docs/design/launcher/prototype is the design it draws).
//!
//! The vault has setlists and songs: they come from it ([`Studio::of`]).
//! What it has no notes for yet (recurring events and their weekly sets,
//! rehearsals, rooms and call times, who leads a song, a set's arrangement
//! and key for each song, records with their mix versions and timestamped
//! comments, the rigs' presets) is the prototype's demo data, kept here
//! behind the same types, so a note property can replace a piece of it at
//! a time without the pages changing.

use std::collections::BTreeMap;

use crate::model::Setlist;

// ── Dates without a calendar crate ───────────────────────────────────────

/// A day, as days since 1970-01-01 (a Thursday).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day(pub i64);

pub const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
pub const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
pub const MONTHS_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

impl Day {
    /// Today, by the clock (UTC).
    #[must_use]
    pub fn today() -> Self {
        let secs = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
        Self(secs.div_euclid(86_400))
    }

    #[must_use]
    pub const fn plus(self, days: i64) -> Self {
        Self(self.0.saturating_add(days))
    }

    /// Days from `from` to this day.
    #[must_use]
    pub const fn since(self, from: Self) -> i64 {
        self.0.saturating_sub(from.0)
    }

    /// 0 Sunday … 6 Saturday.
    #[must_use]
    pub fn weekday(self) -> usize {
        usize::try_from(self.0.saturating_add(4).rem_euclid(7)).unwrap_or(0)
    }

    /// (year, month 1-12, day 1-31): Howard Hinnant's `civil_from_days`.
    #[must_use]
    pub fn civil(self) -> (i64, usize, i64) {
        let z = self.0.saturating_add(719_468);
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        (y, usize::try_from(m).unwrap_or(1), d)
    }

    /// The day `y-m-d`: Howard Hinnant's `days_from_civil`.
    #[must_use]
    pub fn ymd(y: i64, m: i64, d: i64) -> Self {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = if m > 2 { m - 3 } else { m + 9 };
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Self(era * 146_097 + doe - 719_468)
    }

    #[must_use]
    pub fn day_of_month(self) -> i64 {
        self.civil().2
    }

    #[must_use]
    pub fn month(self) -> &'static str {
        MONTHS
            .get(self.civil().1.saturating_sub(1))
            .copied()
            .unwrap_or("")
    }

    #[must_use]
    pub fn wd3(self) -> &'static str {
        WEEKDAYS
            .get(self.weekday())
            .map_or("", |d| d.get(..3).unwrap_or(d))
    }

    /// "Sun Nov 22".
    #[must_use]
    pub fn short(self) -> String {
        format!("{} {} {}", self.wd3(), self.month(), self.day_of_month())
    }

    /// "Today", "Tomorrow", a weekday within the week, else [`Self::short`].
    #[must_use]
    pub fn when(self, today: Self) -> String {
        match self.since(today) {
            0 => "Today".into(),
            1 => "Tomorrow".into(),
            -1 => "Yesterday".into(),
            2..=6 => WEEKDAYS.get(self.weekday()).copied().unwrap_or("").into(),
            _ => self.short(),
        }
    }
}

/// "3:41" or "1:01:43".
#[must_use]
pub fn clock(secs: u32) -> String {
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

// ── Keys round the circle of fifths ───────────────────────────────────────

const FIFTHS: [&str; 12] = [
    "C", "G", "D", "A", "E", "B", "F#", "Db", "Ab", "Eb", "Bb", "F",
];

fn canon(key: &str) -> &str {
    match key {
        "Gb" => "F#",
        "C#" => "Db",
        "G#" => "Ab",
        "D#" => "Eb",
        "A#" => "Bb",
        k => k,
    }
}

/// A key's hue: one step round the circle of fifths a twelfth of the wheel.
#[must_use]
pub fn key_hue(key: &str) -> Option<u16> {
    let root = key.split('→').next().unwrap_or(key).trim();
    FIFTHS
        .iter()
        .position(|k| *k == canon(root))
        .and_then(|i| u16::try_from(i).ok())
        .map(|i| (i * 30 + 20) % 360)
}

fn semis(key: &str) -> Option<i32> {
    Some(match canon(key.trim()) {
        "C" => 0,
        "Db" => 1,
        "D" => 2,
        "Eb" => 3,
        "E" => 4,
        "F" => 5,
        "F#" => 6,
        "G" => 7,
        "Ab" => 8,
        "A" => 9,
        "Bb" => 10,
        "B" => 11,
        _ => return None,
    })
}

/// How far `to` is from `from`, the short way round: −6 … +5.
#[must_use]
pub fn steps(from: &str, to: &str) -> i32 {
    match (semis(from), semis(to)) {
        (Some(a), Some(b)) => (b - a + 18).rem_euclid(12) - 6,
        _ => 0,
    }
}

/// The twelve keys in the circle's order, for a picker.
#[must_use]
pub fn all_keys() -> &'static [&'static str] {
    &FIFTHS
}

/// A hue from a name, for art that has none.
#[must_use]
pub fn name_hue(name: &str) -> u16 {
    let h = name.bytes().fold(7u32, |h, b| {
        h.wrapping_mul(31).wrapping_add(u32::from(b)) % 360
    });
    u16::try_from(h).unwrap_or(0)
}

// ── Live: songs, recurring events, their weekly sets, shows ───────────────

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Song {
    pub title: String,
    pub artist: String,
    pub key: Option<String>,
    pub tempo: Option<u32>,
    pub secs: u32,
    /// Its cover, when its note names one (a `data:` URL or a path).
    pub art: Option<String>,
}

fn song(title: &str, artist: &str, key: &str, tempo: u32, secs: u32) -> Song {
    Song {
        title: title.into(),
        artist: artist.into(),
        key: (!key.is_empty()).then(|| key.into()),
        tempo: (tempo > 0).then_some(tempo),
        secs,
        art: None,
    }
}

/// An event that meets every week (JHM Sunday, Main, …).
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub id: String,
    pub name: String,
    /// 0 Sunday … 6 Saturday.
    pub days: Vec<usize>,
    pub hue: u16,
    /// Songs a set usually has.
    pub size: usize,
    pub room: String,
    pub call: String,
    pub starts: String,
    /// The folder it is in (its tag): Crossroads Church.
    pub folder: String,
    /// The ministry's logo: an image the crate carries (`super::art`).
    pub logo: Option<String>,
}

/// How a set plays one of its songs: its arrangement and key, who leads.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Place {
    pub song: Song,
    /// `None` is the song's own arrangement (Normal).
    pub arrangement: Option<String>,
    /// `None` is the song's own key.
    pub key: Option<String>,
    pub vocal: Option<String>,
    pub guitar: Option<String>,
}

impl Place {
    /// The key it is played in.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref().or(self.song.key.as_deref())
    }
}

/// One meeting of a series: its date and its set.
#[derive(Clone, Debug, PartialEq)]
pub struct Week {
    pub id: String,
    pub series: String,
    pub day: Day,
    pub places: Vec<Place>,
    /// The part you play in it, when you are on it.
    pub mine: Option<String>,
}

impl Week {
    #[must_use]
    pub fn secs(&self) -> u32 {
        self.places.iter().map(|p| p.song.secs).sum()
    }
}

/// A one-off show (a vault setlist that is not a record).
#[derive(Clone, Debug, PartialEq)]
pub struct Show {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub day: Option<Day>,
    /// "May 2026" when it has no day.
    pub month: Option<(String, String)>,
    pub sections: Vec<(Option<String>, Vec<Place>)>,
    pub art: Option<String>,
    pub hue: u16,
    /// Its setlist note's path in the vault, when it has one.
    pub note: Option<String>,
}

impl Show {
    #[must_use]
    pub fn songs(&self) -> usize {
        self.sections.iter().map(|(_, p)| p.len()).sum()
    }

    #[must_use]
    pub fn secs(&self) -> u32 {
        self.sections
            .iter()
            .flat_map(|(_, p)| p)
            .map(|p| p.song.secs)
            .sum()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rehearsal {
    pub week: String,
    pub day: Day,
    pub time: String,
    pub room: String,
}

// ── Studio: records, tracks, versions, comments ───────────────────────────

/// Where a track, or a record, is on its way out.
pub const STAGES: [(&str, Option<u16>); 9] = [
    ("Writing", Some(320)),
    ("Recording", Some(25)),
    ("Editing", Some(290)),
    ("Mixing", Some(65)),
    ("Mastering", Some(215)),
    ("Approval", Some(90)),
    ("Complete", Some(150)),
    ("Released", Some(180)),
    ("Archived", None),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    /// Where in the version, 0…1.
    pub at: f32,
    pub who: String,
    pub text: String,
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Version {
    pub n: u32,
    /// Rough, Edit, Mix, Master.
    pub what: String,
    pub who: String,
    pub when: String,
    pub secs: u32,
    pub comments: Vec<Comment>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    /// The record it is on; `None` is a single.
    pub record: Option<String>,
    pub no: usize,
    pub stage: usize,
    pub when: String,
    pub versions: Vec<Version>,
    pub hue: u16,
    pub art: Option<String>,
    pub cover: bool,
}

impl Track {
    #[must_use]
    pub fn latest(&self) -> Option<&Version> {
        self.versions.last()
    }

    #[must_use]
    pub fn open(&self) -> usize {
        self.versions
            .iter()
            .flat_map(|v| &v.comments)
            .filter(|c| c.open)
            .count()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub what: String,
    pub status: usize,
    pub hue: u16,
    pub art: Option<String>,
    pub org: String,
}

// ── Signal ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    pub name: String,
    pub icon: &'static str,
    pub hue: u16,
    pub preset: Option<String>,
    pub input: String,
    pub loaded: bool,
}

// ── All of it ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct Studio {
    pub today: Day,
    pub series: Vec<Series>,
    pub weeks: Vec<Week>,
    pub shows: Vec<Show>,
    pub rehearsals: Vec<Rehearsal>,
    pub records: Vec<Record>,
    pub tracks: Vec<Track>,
    pub rigs: Vec<Rig>,
    pub library: Vec<(Song, Vec<String>)>,
}

impl Studio {
    #[must_use]
    pub fn series(&self, id: &str) -> Option<&Series> {
        self.series.iter().find(|s| s.id == id)
    }

    #[must_use]
    pub fn week(&self, id: &str) -> Option<&Week> {
        self.weeks.iter().find(|w| w.id == id)
    }

    #[must_use]
    pub fn show(&self, id: &str) -> Option<&Show> {
        self.shows.iter().find(|s| s.id == id)
    }

    #[must_use]
    pub fn record(&self, id: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.id == id)
    }

    #[must_use]
    pub fn track(&self, id: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    #[must_use]
    pub fn tracks_of(&self, record: &str) -> Vec<&Track> {
        let mut t: Vec<&Track> = self
            .tracks
            .iter()
            .filter(|t| t.record.as_deref() == Some(record))
            .collect();
        t.sort_by_key(|t| t.no);
        t
    }

    /// A series' next set, today or later.
    #[must_use]
    pub fn next_of(&self, series: &str) -> Option<&Week> {
        self.weeks
            .iter()
            .filter(|w| w.series == series && w.day >= self.today)
            .min_by_key(|w| w.day)
    }

    /// The sets you are on, next first.
    #[must_use]
    pub fn mine(&self) -> Vec<&Week> {
        let mut w: Vec<&Week> = self
            .weeks
            .iter()
            .filter(|w| w.mine.is_some() && w.day >= self.today)
            .collect();
        w.sort_by_key(|w| w.day);
        w
    }

    /// The art a track shows: its own, else its record's.
    #[must_use]
    pub fn art_of(&self, t: &Track) -> Option<String> {
        t.art.clone().or_else(|| {
            t.record
                .as_deref()
                .and_then(|r| self.record(r))
                .and_then(|r| r.art.clone())
        })
    }

    /// The model: the vault's setlists as shows (and a record's songs as
    /// its tracks), the rest the demo's.
    #[must_use]
    pub fn of(setlists: &[Setlist], today: Day) -> Self {
        let mut s = demo(today);
        for list in setlists {
            let id = crate::model::slug(&list.title);
            let places: Vec<(Option<String>, Vec<Place>)> = list
                .sets
                .iter()
                .map(|set| {
                    let places = set
                        .entries
                        .iter()
                        .map(|e| {
                            let song = e.song.as_ref();
                            Place {
                                song: Song {
                                    title: song
                                        .map_or_else(|| e.label.clone(), |s| s.title.clone()),
                                    artist: song.and_then(|s| s.artist.clone()).unwrap_or_default(),
                                    key: song.and_then(crate::model::Song::keys),
                                    tempo: song.and_then(|s| s.tempo).and_then(|t| {
                                        (t > 0.0).then(|| format!("{t:.0}").parse().ok()).flatten()
                                    }),
                                    secs: song
                                        .and_then(|s| s.length)
                                        .and_then(|l| format!("{l:.0}").parse().ok())
                                        .unwrap_or(0),
                                    art: song.and_then(|s| s.art.clone()),
                                },
                                arrangement: e
                                    .arrangement
                                    .clone()
                                    .filter(|a| !a.eq_ignore_ascii_case("normal")),
                                ..Place::default()
                            }
                        })
                        .collect();
                    (set.name.clone(), places)
                })
                .collect();
            // A record in the vault as a setlist (Crescendum): its songs are the record's tracks.
            if let Some(rec) = s
                .records
                .iter()
                .find(|r| r.title.eq_ignore_ascii_case(&list.title))
                .map(|r| r.id.clone())
            {
                let titles: Vec<String> = places
                    .iter()
                    .flat_map(|(_, p)| p)
                    .map(|p| p.song.title.clone())
                    .collect();
                for (no, title) in titles.iter().enumerate() {
                    if let Some(t) = s
                        .tracks
                        .iter_mut()
                        .find(|t| t.record.as_deref() == Some(rec.as_str()) && t.title == *title)
                    {
                        t.no = no.saturating_add(1);
                    }
                }
                continue;
            }
            // The vault's note for a demo show (its title may say more: "Merry & Bright - CBU Christmas 2026").
            let lower = list.title.to_lowercase();
            if let Some(show) = s
                .shows
                .iter_mut()
                .find(|sh| sh.id == id || lower.starts_with(&sh.title.to_lowercase()))
            {
                show.sections = places;
                show.note = Some(list.path.clone());
            } else {
                s.shows.push(Show {
                    id,
                    title: list.title.clone(),
                    folder: String::new(),
                    day: None,
                    month: list.date.clone().map(|d| (d, String::new())),
                    sections: places,
                    art: None,
                    hue: name_hue(&list.title),
                    note: Some(list.path.clone()),
                });
            }
        }
        s
    }
}

/// The demo: the prototype's data, dated from `today`.
#[allow(clippy::too_many_lines)] // The demo data is a list; it reads best whole.
#[must_use]
pub fn demo(today: Day) -> Studio {
    let pool = vec![
        song("Goodness of God", "Bethel Music", "Ab", 63, 296),
        song("Gratitude", "Brandon Lake", "B", 75, 330),
        song("Build My Life", "Housefires", "G", 68, 290),
        song("Way Maker", "Sinach", "E", 68, 330),
        song("Firm Foundation (He Won't)", "Cody Carnes", "Bb", 70, 312),
        song("Holy Forever", "Chris Tomlin", "A", 72, 330),
        song("Praise", "Elevation Worship", "Bb", 127, 290),
        song("Trust in God", "Elevation Worship", "Ab", 72, 300),
        song("Jireh", "Elevation Worship", "C", 64, 330),
        song("Living Hope", "Phil Wickham", "Db", 72, 330),
        song("This Is Amazing Grace", "Phil Wickham", "A", 103, 280),
        song("See A Victory", "Elevation Worship", "B", 72, 330),
        song("King of Kings", "Hillsong Worship", "D", 68, 300),
        song("Graves Into Gardens", "Elevation Worship", "B", 72, 300),
        song("Same God", "Elevation Worship", "Db", 72, 330),
        song(
            "Joy to the World (Unspeakable Joy)",
            "Chris Tomlin",
            "D",
            128,
            250,
        ),
    ];
    let church = "Crossroads Church".to_owned();
    let ser = |id: &str,
               name: &str,
               days: &[usize],
               hue: u16,
               size: usize,
               room: &str,
               call: &str,
               starts: &str,
               logo: &str| Series {
        id: id.into(),
        name: name.into(),
        days: days.to_vec(),
        hue,
        size,
        room: room.into(),
        call: call.into(),
        starts: starts.into(),
        folder: church.clone(),
        logo: (!logo.is_empty()).then(|| logo.into()),
    };
    let series = vec![
        ser(
            "jhms",
            "JHM Sunday",
            &[0],
            285,
            5,
            "Room 210",
            "7:45",
            "9:00",
            "jhm",
        ),
        ser(
            "main",
            "Main",
            &[0, 3],
            45,
            5,
            "Worship Center",
            "8:30",
            "10:30",
            "main",
        ),
        ser(
            "hsm",
            "HSM",
            &[2],
            215,
            4,
            "The Loft",
            "5:45",
            "7:00",
            "hsm",
        ),
        ser(
            "jhm",
            "JHM",
            &[3],
            150,
            4,
            "Room 210",
            "6:00",
            "7:00",
            "jhm",
        ),
        ser("cya", "CYA", &[4], 345, 4, "Chapel", "6:15", "7:30", "cya"),
    ];
    // Six weeks back and two ahead: the past full, this week partly planned, later empty.
    let mut weeks = Vec::new();
    let leads = ["Mia", "Jake", "Sam"];
    let arrangements = [
        None,
        Some("Big Intro"),
        None,
        Some("Shortened"),
        Some("Acoustic"),
    ];
    for (si, sr) in series.iter().enumerate() {
        let mut n = 0usize;
        for offset in -42..=17i64 {
            let day = today.plus(offset);
            if !sr.days.contains(&day.weekday()) {
                continue;
            }
            let ahead = day.since(today);
            let filled = if ahead < 0 {
                sr.size
            } else if ahead < 7 {
                match sr.id.as_str() {
                    "cya" | "main" if ahead < 4 => sr.size,
                    "jhms" => sr.size.saturating_sub(1),
                    "hsm" => 2,
                    "main" => 3,
                    _ => 0,
                }
            } else {
                0
            };
            let places = (0..filled)
                .map(|k| {
                    let i = n
                        .wrapping_mul(5)
                        .wrapping_add(k.wrapping_mul(3))
                        .wrapping_add(si.wrapping_mul(7))
                        % pool.len();
                    Place {
                        song: pool
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| song("", "", "", 0, 0)),
                        arrangement: arrangements
                            .get(k.wrapping_add(n) % arrangements.len())
                            .copied()
                            .flatten()
                            .map(Into::into),
                        key: (k == 0 && ahead >= 0 && sr.id == "main").then(|| "C".to_owned()),
                        vocal: leads
                            .get(k.wrapping_add(n) % leads.len())
                            .map(|s| (*s).to_owned()),
                        guitar: (k % 2 == 1).then(|| "Cody".to_owned()),
                    }
                })
                .collect();
            let (y, m, d) = day.civil();
            let mine = match sr.id.as_str() {
                "jhms" => Some("Guitar".to_owned()),
                "cya" if (0..7).contains(&ahead) => Some("Guitar".to_owned()),
                _ => None,
            };
            weeks.push(Week {
                id: format!("{}@{y}-{m:02}-{d:02}", sr.id),
                series: sr.id.clone(),
                day,
                places,
                mine,
            });
            n = n.saturating_add(1);
        }
    }
    let rehearsals = series
        .iter()
        .find(|s| s.id == "jhms")
        .and_then(|_| {
            weeks
                .iter()
                .filter(|w| w.series == "jhms" && w.day > today)
                .min_by_key(|w| w.day)
        })
        .map(|w| Rehearsal {
            week: w.id.clone(),
            day: w.day.plus(-1),
            time: "10:00 AM".into(),
            room: "Room 210".into(),
        })
        .into_iter()
        .collect();
    let shows = vec![
        Show {
            id: "merry-bright-cbu-christmas-2026".into(),
            title: "Merry & Bright".into(),
            folder: "CBU".into(),
            day: Some(Day::ymd(2026, 12, 12)),
            month: None,
            sections: Vec::new(),
            art: None,
            hue: 150,
            note: None,
        },
        Show {
            id: "jts-senior-recital".into(),
            title: "JT's Senior Recital".into(),
            folder: "Recitals".into(),
            day: None,
            month: Some(("May".into(), "2026".into())),
            sections: Vec::new(),
            art: Some("jt".into()),
            hue: 20,
            note: None,
        },
    ];
    let rec = |id: &str,
               title: &str,
               artist: &str,
               what: &str,
               status: usize,
               hue: u16,
               art: &str,
               org: &str| Record {
        id: id.into(),
        title: title.into(),
        artist: artist.into(),
        what: what.into(),
        status,
        hue,
        art: (!art.is_empty()).then(|| art.into()),
        org: org.into(),
    };
    let records = vec![
        rec(
            "cr",
            "Crescendum",
            "Crescendum",
            "Album",
            3,
            285,
            "crescendum",
            "FastTrackStudio",
        ),
        rec(
            "png",
            "PNG Worship Collective Album",
            "PNG Worship Collective",
            "Album",
            2,
            150,
            "",
            "Tom Brooks Music",
        ),
        rec(
            "mtl",
            "Montreal Benefit Album",
            "Various artists",
            "Benefit album",
            1,
            5,
            "",
            "Tom Brooks Music",
        ),
        rec(
            "joi",
            "A Journey of Immigrants",
            "Andres Jaramillo",
            "Album",
            5,
            45,
            "",
            "Tom Brooks Music",
        ),
    ];
    let list = |rec: &str, artist: &str, hue: u16, tracks: &[(&str, usize, &str)]| -> Vec<Track> {
        tracks
            .iter()
            .enumerate()
            .map(|(k, (title, stage, when))| Track {
                id: format!("t:{title}"),
                title: (*title).into(),
                artist: artist.into(),
                record: Some(rec.into()),
                no: k.saturating_add(1),
                stage: *stage,
                when: (*when).into(),
                versions: Vec::new(),
                hue,
                art: None,
                cover: false,
            })
            .collect()
    };
    let mut tracks = Vec::new();
    tracks.extend(list(
        "cr",
        "Crescendum",
        285,
        &[
            ("Intro", 6, "Mon"),
            ("Set in Stone", 5, "Last week"),
            ("Unbreakable", 3, "2h"),
            ("Chained Expectations", 3, "5h"),
            ("Heavify", 2, "Yesterday"),
            ("It Knows My Name", 2, "Nov 10"),
            ("Kornesque", 1, "Nov 8"),
            ("The Ballad", 1, "Yesterday"),
            ("The Plague", 1, "Nov 6"),
            ("Empty Focus", 0, "Oct 30"),
        ],
    ));
    tracks.extend(list(
        "png",
        "PNG Worship Collective",
        150,
        &[
            ("All That I Am", 3, "May 26"),
            ("Lord of the Fight", 2, "May 26"),
            ("Dwelling Place", 2, "May 20"),
            ("Presence", 2, "May 20"),
            ("Life Giving Word", 1, "May 18"),
            ("He's Been Good", 2, "May 18"),
            ("I Know a Name", 2, "May 12"),
            ("Seek Your Face", 2, "May 12"),
            ("Hannah's Song", 2, "May 10"),
            ("Reason Why", 2, "May 10"),
            ("Goodness of God (RK version)", 3, "May 8"),
            ("Unity", 2, "May 8"),
        ],
    ));
    tracks.extend(list(
        "mtl",
        "Various artists",
        5,
        &[
            ("Courage", 3, "Aug 4"),
            ("Radio", 3, "Aug 4"),
            ("Too Young", 1, "Jul 9"),
            ("Where to Go", 2, "Jul 9"),
            ("You and I", 2, "Jul 9"),
        ],
    ));
    tracks.extend(list(
        "joi",
        "Andres Jaramillo",
        45,
        &[
            ("Toccata", 5, "May 8"),
            ("Ancestro", 5, "May 8"),
            ("La Zacapaneca", 5, "May 8"),
            ("Five Colombian Pieces", 5, "May 8"),
            ("Luz Entre Aguas", 5, "May 8"),
            ("El Intachable", 5, "May 8"),
            ("Etudes for Immigrants", 5, "May 8"),
        ],
    ));
    for (title, artist, stage, when, hue, art, cover) in [
        (
            "Devour",
            "Beautifully Violent",
            3usize,
            "Jul 6",
            330u16,
            "bviolent",
            false,
        ),
        ("Fire", "JT Wright", 1, "Mon", 40, "", false),
        (
            "Goodness of God",
            "Ron Kenoly",
            4,
            "Oct 12",
            0,
            "ghost",
            true,
        ),
        ("Drive To LA", "Nights Out", 4, "Last week", 10, "", false),
    ] {
        tracks.push(Track {
            id: format!("t:{title}"),
            title: title.into(),
            artist: artist.into(),
            record: None,
            no: 0,
            stage,
            when: when.into(),
            versions: Vec::new(),
            hue,
            art: (!art.is_empty()).then(|| art.into()),
            cover,
        });
    }
    // Versions: as many as its stage has had, the latest by whoever bounced it.
    for (i, t) in tracks.iter_mut().enumerate() {
        let what = match t.stage {
            4.. => "Master",
            3 => "Mix",
            2 => "Edit",
            _ => "Rough",
        };
        let n = u32::try_from(t.stage.clamp(1, 4)).unwrap_or(1);
        let secs =
            170u32.saturating_add(u32::try_from(t.title.len().wrapping_mul(13) % 160).unwrap_or(0));
        t.versions = (1..=n)
            .map(|v| Version {
                n: v,
                what: what.into(),
                who: if (v.wrapping_add(u32::try_from(i).unwrap_or(0))) % 2 == 0 {
                    "Cody".into()
                } else {
                    "Jake".into()
                },
                when: if v == n {
                    t.when.clone()
                } else {
                    format!("{} days ago", (n - v).saturating_mul(4).saturating_add(3))
                },
                secs,
                comments: Vec::new(),
            })
            .collect();
    }
    let comments: BTreeMap<&str, Vec<Comment>> = BTreeMap::from([
        (
            "Unbreakable",
            vec![
                Comment {
                    at: 0.38,
                    who: "Jake".into(),
                    text: "Guitars eat the vocal in the pre-chorus".into(),
                    open: true,
                },
                Comment {
                    at: 0.66,
                    who: "Mia".into(),
                    text: "The delay throw here is perfect".into(),
                    open: false,
                },
                Comment {
                    at: 0.81,
                    who: "Sam".into(),
                    text: "Ride the bass up into the last chorus".into(),
                    open: true,
                },
            ],
        ),
        (
            "Chained Expectations",
            vec![Comment {
                at: 0.52,
                who: "Mia".into(),
                text: "Is the bridge D/F# or Bm? The demo walks the bass down.".into(),
                open: true,
            }],
        ),
        (
            "Heavify",
            vec![Comment {
                at: 0.14,
                who: "Sam".into(),
                text: "Flam on the snare here".into(),
                open: true,
            }],
        ),
        (
            "Courage",
            vec![Comment {
                at: 0.3,
                who: "Rob".into(),
                text: "Can the acoustic come up in verse 2?".into(),
                open: true,
            }],
        ),
    ]);
    for t in &mut tracks {
        if let (Some(c), Some(v)) = (comments.get(t.title.as_str()), t.versions.last_mut()) {
            v.comments.clone_from(c);
        }
    }
    let rig =
        |name: &str, icon: &'static str, hue: u16, preset: &str, input: &str, loaded: bool| Rig {
            name: name.into(),
            icon,
            hue,
            preset: (!preset.is_empty()).then(|| preset.into()),
            input: input.into(),
            loaded,
        };
    let rigs = vec![
        rig(
            "Drums",
            "drums",
            70,
            "Recital kit",
            "From the session",
            false,
        ),
        rig("Perc", "perc", 110, "Shakers and congas", "SPD-SX", false),
        rig("Bass", "bass", 305, "Royal Blood fuzz", "Input 2", false),
        rig("Guitar", "guitar", 30, "CYA", "Input 1 · Hi-Z", true),
        rig(
            "Acoustic",
            "acoustic",
            190,
            "Worship, warm",
            "Input 3 · DI",
            false,
        ),
        rig("Keys", "keys", 255, "JHM Sunday", "Kontrol S88", false),
    ];
    let library = pool
        .iter()
        .map(|s| {
            let mut tags = vec!["worship".to_owned(), "contemporary".to_owned()];
            tags.push(if s.tempo.unwrap_or(0) > 100 {
                "upbeat".into()
            } else {
                "ballad".into()
            });
            (s.clone(), tags)
        })
        .collect();
    Studio {
        today,
        series,
        weeks,
        shows,
        rehearsals,
        records,
        tracks,
        rigs,
        library,
    }
}
