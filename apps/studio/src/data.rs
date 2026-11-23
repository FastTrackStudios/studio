//! The made-up content: the prototype's sets, songs, weekly events, rigs,
//! records and reviews, all in memory. Nothing here changes; what the app
//! lets you change (keys, arrangements, rehearsals, comments, statuses) is
//! state in [`crate::state`].

use std::collections::HashMap;
use std::sync::LazyLock;

use dioxus::prelude::*;

use crate::icons::I;

// ── Days ────────────────────────────────────────────────────────────────────

/// A calendar day, as days since 1970-01-01.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Day(pub i32);

pub const DAY: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
pub const MON: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
pub const MONTH: [&str; 12] = [
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
    /// The day of a civil date (`m` 1–12).
    pub const fn ymd(y: i32, m: u32, d: u32) -> Day {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = (y - era * 400) as u32;
        let mp = if m > 2 { m - 3 } else { m + 9 };
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        Day(era * 146097 + doe as i32 - 719468)
    }

    /// Year, month (1–12) and day of the month.
    pub fn civil(self) -> (i32, u32, u32) {
        let z = self.0 + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe as i32 + era * 400 + i32::from(m <= 2);
        (y, m, d)
    }

    /// 0 for Sunday.
    pub fn weekday(self) -> usize {
        (self.0 + 4).rem_euclid(7) as usize
    }
    pub fn month0(self) -> usize {
        self.civil().1 as usize - 1
    }
    pub fn mday(self) -> u32 {
        self.civil().2
    }
    pub fn year(self) -> i32 {
        self.civil().0
    }
    pub fn iso(self) -> String {
        let (y, m, d) = self.civil();
        format!("{y:04}-{m:02}-{d:02}")
    }
    pub fn plus(self, n: i32) -> Day {
        Day(self.0 + n)
    }
    /// Days from today: 0 today, negative before.
    pub fn from_today(self) -> i32 {
        self.0 - TODAY.0
    }
    pub fn dow3(self) -> &'static str {
        &DAY[self.weekday()][..3]
    }
    /// "Thu Nov 19".
    pub fn short(self) -> String {
        format!("{} {} {}", self.dow3(), MON[self.month0()], self.mday())
    }
    /// Today, Tomorrow, Yesterday, a weekday this week, else the short date.
    pub fn when(self) -> String {
        match self.from_today() {
            0 => "Today".into(),
            1 => "Tomorrow".into(),
            -1 => "Yesterday".into(),
            n if n > 0 && n < 7 => DAY[self.weekday()].into(),
            _ => self.short(),
        }
    }
}

/// Thursday, November 19 2026: the prototype's today.
pub const TODAY: Day = Day::ymd(2026, 11, 19);

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn dur(s: f64) -> String {
    let s = s.round() as i64;
    let (h, m, x) = (s / 3600, s % 3600 / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{x:02}")
    } else {
        format!("{m}:{x:02}")
    }
}

// ── Keys ────────────────────────────────────────────────────────────────────

pub const FIFTHS: [&str; 12] = [
    "C", "G", "D", "A", "E", "B", "F#", "Db", "Ab", "Eb", "Bb", "F",
];

fn alias(k: &str) -> &str {
    match k {
        "Gb" => "F#",
        "C#" => "Db",
        "G#" => "Ab",
        "D#" => "Eb",
        "A#" => "Bb",
        k => k,
    }
}

/// A key's hue round the circle of fifths.
pub fn hue(k: &str) -> Option<u32> {
    let k = k.split('→').next().unwrap_or(k);
    FIFTHS
        .iter()
        .position(|f| *f == alias(k))
        .map(|i| (i as u32 * 30 + 20) % 360)
}

fn semi(k: &str) -> i32 {
    match k {
        "C" => 0,
        "C#" | "Db" => 1,
        "D" => 2,
        "D#" | "Eb" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "Gb" => 6,
        "G" => 7,
        "G#" | "Ab" => 8,
        "A" => 9,
        "A#" | "Bb" => 10,
        "B" => 11,
        _ => 0,
    }
}

/// Semitones from one key to another, the short way (−6 to +5).
pub fn steps(from: &str, to: &str) -> i32 {
    let from = from.split('→').next().unwrap_or(from);
    (semi(to) - semi(from) + 18).rem_euclid(12) - 6
}

/// A hue from any text, the same every time.
pub fn str_hue(t: &str) -> u32 {
    t.chars().fold(7u32, |h, c| (h * 31 + c as u32) % 360)
}

// ── Songs and sets ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Song {
    pub title: &'static str,
    pub artist: Option<&'static str>,
    pub key: Option<&'static str>,
    pub tempo: Option<u32>,
    pub len: Option<u32>,
    pub ready: &'static str,
}

/// A song read at run time (a setlist file's) as a [`Song`]: its title
/// kept for the life of the app, once per title.
#[cfg_attr(not(feature = "drive"), allow(dead_code))]
pub fn song_named(title: &str) -> Song {
    thread_local! {
        static TITLES: std::cell::RefCell<HashMap<String, &'static str>> =
            std::cell::RefCell::new(HashMap::new());
    }
    let title = TITLES.with(|t| {
        *t.borrow_mut()
            .entry(title.to_string())
            .or_insert_with(|| Box::leak(title.to_string().into_boxed_str()))
    });
    s(title, None, None, None, None, "")
}

const fn s(
    title: &'static str,
    artist: Option<&'static str>,
    key: Option<&'static str>,
    tempo: Option<u32>,
    len: Option<u32>,
    ready: &'static str,
) -> Song {
    Song {
        title,
        artist,
        key,
        tempo,
        len,
        ready,
    }
}

/// A one-off show: a named set in acts.
pub struct Show {
    pub id: &'static str,
    pub title: &'static str,
    /// The month and year (or day) its date tile shows.
    pub hue: u32,
    pub acts: Vec<(&'static str, Vec<Song>)>,
}

impl Show {
    pub fn songs(&self) -> Vec<Song> {
        self.acts.iter().flat_map(|a| a.1.iter().copied()).collect()
    }
}

const N: Option<&str> = None;

pub static SHOWS: LazyLock<Vec<Show>> = LazyLock::new(|| {
    let a = Some;
    vec![
        Show {
            id: "jt",
            title: "JT's Senior Recital",
            hue: 20,
            acts: vec![
                (
                    "The Originals",
                    vec![
                        s(
                            "DTLA Cinematic Intro",
                            a("Nights Out"),
                            a("C"),
                            Some(160),
                            Some(123),
                            "cltm",
                        ),
                        s(
                            "Drive To LA",
                            a("Nights Out"),
                            a("C→Db"),
                            Some(160),
                            Some(197),
                            "cltm",
                        ),
                        s(
                            "For You",
                            a("Nights Out"),
                            a("Ab"),
                            Some(105),
                            Some(193),
                            "cltm",
                        ),
                        s(
                            "Heart Isn't Ready",
                            a("Nights Out"),
                            a("Bb"),
                            Some(107),
                            Some(203),
                            "cltm",
                        ),
                        s(
                            "No One Knows",
                            a("Queens of the Stone Age"),
                            N,
                            None,
                            Some(257),
                            "lt",
                        ),
                        s(
                            "Hey!",
                            a("Nights Out & Ryce"),
                            a("E"),
                            Some(87),
                            Some(251),
                            "cltm",
                        ),
                    ],
                ),
                (
                    "Royal Blood",
                    vec![
                        s("Sleeptalker", a("Royal Blood"), N, None, Some(191), "lt"),
                        s("Figure It Out", a("Royal Blood"), N, None, Some(189), "lt"),
                        s(
                            "Seven Nation Army",
                            a("The White Stripes"),
                            N,
                            None,
                            Some(231),
                            "lt",
                        ),
                        s("Sad But True", a("Metallica"), N, None, Some(327), "lt"),
                        s(
                            "Out Of The Black",
                            a("Royal Blood"),
                            N,
                            None,
                            Some(327),
                            "lt",
                        ),
                    ],
                ),
                (
                    "The Fun Part",
                    vec![
                        s(
                            "She's American",
                            a("The 1975"),
                            a("Db"),
                            Some(116),
                            Some(267),
                            "cltm",
                        ),
                        s(
                            "WORST PART OF YOU",
                            a("bilmuri"),
                            a("B"),
                            Some(183),
                            Some(176),
                            "cltm",
                        ),
                        s("Fire", a("JT Wright"), a("D"), Some(120), Some(206), "cltm"),
                        s(
                            "Not Too Much Longer (Live)",
                            a("Cody Wright"),
                            N,
                            None,
                            Some(261),
                            "lt",
                        ),
                        s(
                            "Know Me (Extended)",
                            a("The Band CAMINO"),
                            N,
                            None,
                            Some(304),
                            "clt",
                        ),
                    ],
                ),
            ],
        },
        Show {
            id: "mb",
            title: "Merry & Bright",
            hue: 150,
            acts: vec![
                (
                    "Presidents Songs",
                    vec![
                        s(
                            "All I Want for Christmas Is You",
                            a("Mariah Carey"),
                            a("G"),
                            Some(150),
                            None,
                            "cl",
                        ),
                        s("Feliz Navidad", N, N, None, None, ""),
                        s(
                            "Joy to the World (Unspeakable Joy)",
                            a("Chris Tomlin"),
                            a("D"),
                            Some(128),
                            None,
                            "cl",
                        ),
                    ],
                ),
                (
                    "Tree Lighting Concert Set",
                    vec![
                        s("Last Christmas", N, N, None, None, ""),
                        s("Rockin' Around the Christmas Tree", N, N, None, None, ""),
                        s(
                            "Silent Night",
                            a("for KING & COUNTRY"),
                            a("A"),
                            Some(83),
                            None,
                            "cl",
                        ),
                        s("Sleigh Ride", a("Tori Kelly"), a("B"), Some(86), None, "cl"),
                        s(
                            "This Christmas",
                            a("Donny Hathaway"),
                            a("F"),
                            Some(93),
                            None,
                            "cl",
                        ),
                        s("What Christmas Means", N, N, None, None, ""),
                    ],
                ),
            ],
        },
    ]
});

pub fn show(id: &str) -> Option<&'static Show> {
    SHOWS.iter().find(|s| s.id == id)
}

/// A weekly event: it meets on its days, and each meeting's set has only
/// its date for a name.
pub struct Series {
    pub id: &'static str,
    pub name: &'static str,
    pub days: &'static [usize],
    pub hue: u32,
    pub size: usize,
    /// What a day of it is called, where it is not just the weekday.
    pub note: Option<(usize, &'static str)>,
}

pub static SERIES: [Series; 5] = [
    Series {
        id: "jhms",
        name: "JHM Sunday",
        days: &[0],
        hue: 285,
        size: 5,
        note: None,
    },
    Series {
        id: "main",
        name: "Main",
        days: &[0, 3],
        hue: 45,
        size: 5,
        note: Some((3, "Wednesday night")),
    },
    Series {
        id: "hsm",
        name: "HSM",
        days: &[2],
        hue: 215,
        size: 4,
        note: None,
    },
    Series {
        id: "jhm",
        name: "JHM",
        days: &[3],
        hue: 150,
        size: 4,
        note: None,
    },
    Series {
        id: "cya",
        name: "CYA",
        days: &[4],
        hue: 345,
        size: 4,
        note: None,
    },
];

pub fn series(id: &str) -> Option<&'static Series> {
    SERIES.iter().find(|s| s.id == id)
}

impl Series {
    /// Where it meets, when the band is called, when it starts.
    pub fn place(&self) -> Option<Place> {
        let (room, starts, call) = match self.id {
            "main" => ("Worship Center", "10:30", "8:30"),
            "jhms" => ("Room 210", "9:00", "7:45"),
            "jhm" => ("Room 210", "7:00", "6:00"),
            "hsm" => ("The Loft", "7:00", "5:45"),
            "cya" => ("Chapel", "7:30", "6:15"),
            _ => return None,
        };
        Some(Place { room, starts, call })
    }
}

#[derive(Clone, Copy)]
pub struct Place {
    pub room: &'static str,
    pub starts: &'static str,
    pub call: &'static str,
}

/// One meeting of a weekly event, and its set.
pub struct Occ {
    pub id: String,
    pub series: &'static Series,
    pub date: Day,
    pub songs: Vec<Song>,
}

impl Occ {
    pub fn name(&self) -> String {
        match self.series.note {
            Some((d, n)) if d == self.date.weekday() => format!("{}, {}", self.series.name, n),
            _ => self.series.name.into(),
        }
    }
}

/// The songs the weekly sets draw from (a worship rotation).
pub static POOL: LazyLock<Vec<Song>> = LazyLock::new(|| {
    let a = Some;
    let r = "";
    vec![
        s(
            "Goodness of God",
            a("Bethel Music"),
            a("Ab"),
            Some(63),
            Some(296),
            r,
        ),
        s(
            "Gratitude",
            a("Brandon Lake"),
            a("B"),
            Some(75),
            Some(330),
            r,
        ),
        s(
            "Build My Life",
            a("Housefires"),
            a("G"),
            Some(68),
            Some(290),
            r,
        ),
        s("Way Maker", a("Sinach"), a("E"), Some(68), Some(330), r),
        s(
            "Firm Foundation (He Won't)",
            a("Cody Carnes"),
            a("Bb"),
            Some(70),
            Some(312),
            r,
        ),
        s(
            "Holy Forever",
            a("Chris Tomlin"),
            a("A"),
            Some(72),
            Some(330),
            r,
        ),
        s(
            "Praise",
            a("Elevation Worship"),
            a("Bb"),
            Some(127),
            Some(290),
            r,
        ),
        s(
            "Trust in God",
            a("Elevation Worship"),
            a("Ab"),
            Some(72),
            Some(300),
            r,
        ),
        s(
            "Jireh",
            a("Elevation Worship"),
            a("C"),
            Some(64),
            Some(330),
            r,
        ),
        s(
            "Living Hope",
            a("Phil Wickham"),
            a("Db"),
            Some(72),
            Some(330),
            r,
        ),
        s(
            "This Is Amazing Grace",
            a("Phil Wickham"),
            a("A"),
            Some(103),
            Some(280),
            r,
        ),
        s(
            "See A Victory",
            a("Elevation Worship"),
            a("B"),
            Some(72),
            Some(330),
            r,
        ),
        s(
            "King of Kings",
            a("Hillsong Worship"),
            a("D"),
            Some(68),
            Some(300),
            r,
        ),
        s(
            "Graves Into Gardens",
            a("Elevation Worship"),
            a("B"),
            Some(72),
            Some(300),
            r,
        ),
        s(
            "Same God",
            a("Elevation Worship"),
            a("Db"),
            Some(72),
            Some(330),
            r,
        ),
        s(
            "Joy to the World (Unspeakable Joy)",
            a("Chris Tomlin"),
            a("D"),
            Some(128),
            Some(250),
            r,
        ),
    ]
});

/// How far along this week's sets are (the rest of the future is empty).
fn planned(id: &str) -> usize {
    match id {
        "cya_2026-11-19" | "jhms_2026-11-22" => 4,
        "main_2026-11-22" => 5,
        "hsm_2026-11-24" => 2,
        "main_2026-11-25" => 3,
        _ => 0,
    }
}

/// Every meeting from Oct 18 to Dec 6, each with its set.
pub static OCC: LazyLock<Vec<Occ>> = LazyLock::new(|| {
    let mut out: Vec<Occ> = Vec::new();
    for (si, sr) in SERIES.iter().enumerate() {
        let mut d = Day::ymd(2026, 10, 18);
        let mut n = 0;
        while d <= Day::ymd(2026, 12, 6) {
            if sr.days.contains(&d.weekday()) {
                let id = format!("{}_{}", sr.id, d.iso());
                let filled = if d.from_today() < 0 {
                    sr.size
                } else {
                    planned(&id)
                };
                let wed = if d.weekday() == 3 { 2 } else { 0 };
                let songs = (0..filled)
                    .map(|k| POOL[(n * 5 + k * 3 + si * 7 + wed) % POOL.len()])
                    .collect();
                out.push(Occ {
                    id,
                    series: sr,
                    date: d,
                    songs,
                });
                n += 1;
            }
            d = d.plus(1);
        }
    }
    out
});

pub fn occ(id: &str) -> Option<&'static Occ> {
    OCC.iter().find(|o| o.id == id)
}

// ── Rigs ────────────────────────────────────────────────────────────────────

pub struct Rig {
    pub name: &'static str,
    pub icon: I,
    pub hue: u32,
    pub preset: &'static str,
    pub input: &'static str,
    pub loaded: bool,
}

pub static RIGS: [Rig; 6] = [
    Rig {
        name: "Drums",
        icon: I::Drums,
        hue: 70,
        preset: "Recital kit",
        input: "From the session",
        loaded: false,
    },
    Rig {
        name: "Perc",
        icon: I::Perc,
        hue: 110,
        preset: "Shakers and congas",
        input: "SPD-SX",
        loaded: false,
    },
    Rig {
        name: "Bass",
        icon: I::Bass,
        hue: 305,
        preset: "Royal Blood fuzz",
        input: "Input 2",
        loaded: false,
    },
    Rig {
        name: "Guitar",
        icon: I::Guitar,
        hue: 30,
        preset: "CYA, Thu Nov 19",
        input: "Input 1 · Hi-Z",
        loaded: true,
    },
    Rig {
        name: "Acoustic",
        icon: I::Acoustic,
        hue: 190,
        preset: "Worship, warm",
        input: "Input 3 · DI",
        loaded: false,
    },
    Rig {
        name: "Keys",
        icon: I::Keys,
        hue: 255,
        preset: "JHM Sunday",
        input: "Kontrol S88",
        loaded: false,
    },
];

pub fn rig(name: &str) -> Option<&'static Rig> {
    RIGS.iter().find(|r| r.name == name)
}

pub fn loaded_rig() -> Option<&'static Rig> {
    RIGS.iter().find(|r| r.loaded)
}

/// What you play at a set: CYA today, JHM Sunday every week.
pub fn mine(occ_id: &str) -> Option<&'static Rig> {
    const MINE: [&str; 9] = [
        "cya_2026-11-19",
        "jhms_2026-10-18",
        "jhms_2026-10-25",
        "jhms_2026-11-01",
        "jhms_2026-11-08",
        "jhms_2026-11-15",
        "jhms_2026-11-22",
        "jhms_2026-11-29",
        "jhms_2026-12-06",
    ];
    MINE.contains(&occ_id).then(|| rig("Guitar")).flatten()
}

// ── Art ─────────────────────────────────────────────────────────────────────

/// Real cover art where there is some.
pub fn art(id: &str) -> Option<Asset> {
    match id {
        "jt" | "show" => Some(asset!("/assets/covers/jt.png")),
        "cr" => Some(asset!("/assets/covers/crescendum.png")),
        "t:Devour" => Some(asset!("/assets/covers/bviolent.png")),
        "t:Goodness of God" => Some(asset!("/assets/covers/ghost.png")),
        _ => None,
    }
}

/// The ministries' logos, for their weekly sets.
pub fn logo(series_id: &str) -> Option<Asset> {
    match series_id {
        "main" => Some(asset!("/assets/covers/main.png")),
        "cya" => Some(asset!("/assets/covers/cya.png")),
        "hsm" => Some(asset!("/assets/covers/hsm.png")),
        "jhm" | "jhms" => Some(asset!("/assets/covers/jhm.png")),
        _ => None,
    }
}

/// JT's own songs carry his cover.
pub const OWN: [&str; 4] = [
    "Nights Out",
    "Nights Out & Ryce",
    "JT Wright",
    "Cody Wright",
];
pub const JT_COVER: Asset = asset!("/assets/covers/jt.png");

// ── Who leads, arrangements ─────────────────────────────────────────────────

pub const ME: &str = "Cody";

/// Who leads a song in a set: its lead vocal and lead guitar.
pub fn leads_of(o: &Occ, i: usize, x: &Song) -> (Option<&'static str>, Option<&'static str>) {
    if o.id == "cya_2026-11-19" {
        const L: [(Option<&str>, Option<&str>); 4] = [
            (Some("Mia"), None),
            (Some("Jake"), Some("Cody")),
            (Some("Mia"), Some("Cody")),
            (Some("Sam"), None),
        ];
        return L.get(i).copied().unwrap_or((None, None));
    }
    let v = ["Mia", "Jake", "Sam"][(i + o.date.mday() as usize) % 3];
    let g = (x.tempo.unwrap_or(0) > 100 || i % 2 == 1).then_some("Cody");
    (Some(v), g)
}

/// The arrangements a song can be played in, and how much each adds.
pub const ARRS: [(&str, i32); 5] = [
    ("Normal", 0),
    ("Shortened", -70),
    ("Big Intro", 25),
    ("Acoustic", -10),
    ("Extended", 45),
];

/// The arrangement a set plays a song in, before anyone changes it.
pub fn arr_default(o: &Occ, i: usize) -> &'static str {
    if o.id == "cya_2026-11-19" {
        return ["Normal", "Big Intro", "Shortened", "Normal"]
            .get(i)
            .copied()
            .unwrap_or("Normal");
    }
    ["Normal", "Big Intro", "Normal", "Shortened", "Acoustic"][(i + o.date.mday() as usize) % 5]
}

/// Keys a set moved before anyone opened it.
pub fn key_default(occ_id: &str, i: usize) -> Option<&'static str> {
    match (occ_id, i) {
        ("main_2026-11-22", 0) => Some("C"),
        ("main_2026-11-22", 1) => Some("E"),
        _ => None,
    }
}

/// A week's set is two sections: the Top Set's two songs, the Bottom Set's rest.
pub fn sections_of(o: &Occ) -> [(&'static str, usize, usize); 2] {
    let n = o.series.size.max(o.songs.len());
    [("Top Set", 0, 2.min(n)), ("Bottom Set", 2.min(n), n)]
}

pub const PEOPLE: [(&str, u32); 4] = [("Cody", 30), ("Jake", 200), ("Mia", 320), ("Sam", 140)];

pub fn person_hue(who: &str) -> u32 {
    PEOPLE.iter().find(|p| p.0 == who).map_or(0, |p| p.1)
}

/// Rehearsals booked for a set.
#[derive(Clone, PartialEq)]
pub struct Rehearsal {
    pub set: String,
    pub date: Day,
    pub time: String,
    pub room: String,
}

pub fn first_rehearsals() -> Vec<Rehearsal> {
    vec![Rehearsal {
        set: "jhms_2026-11-22".into(),
        date: Day::ymd(2026, 11, 21),
        time: "10:00 AM".into(),
        room: "Room 210".into(),
    }]
}

// ── The library ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub struct LibSong {
    pub song: Song,
    pub tags: Vec<&'static str>,
    pub energy: u8,
}

const ROCK: [&str; 4] = [
    "Royal Blood",
    "Metallica",
    "Queens of the Stone Age",
    "The White Stripes",
];

pub static LIB: LazyLock<Vec<LibSong>> = LazyLock::new(|| {
    let mut out: Vec<LibSong> = Vec::new();
    let mut add = |x: Song, tags: Vec<&'static str>| {
        if out.iter().any(|y| y.song.title == x.title) {
            return;
        }
        let energy = x.tempo.map_or(3, |t| {
            ((t as f64 - 50.0) / 22.0).round().clamp(1.0, 5.0) as u8
        });
        out.push(LibSong {
            song: x,
            tags,
            energy,
        });
    };
    for x in POOL.iter() {
        let mut tags = vec!["worship", "contemporary"];
        tags.push(if x.tempo.unwrap_or(0) > 100 {
            "upbeat"
        } else {
            "ballad"
        });
        if x.artist == Some("Chris Tomlin") {
            tags.push("hymns");
        }
        add(
            Song {
                ready: "cltm",
                ..*x
            },
            tags,
        );
    }
    for x in show("mb").map(Show::songs).unwrap_or_default() {
        let mut tags = vec!["christmas"];
        if x.artist == Some("Chris Tomlin") {
            tags.push("worship");
        }
        if ["Silent Night", "Joy to the World (Unspeakable Joy)"].contains(&x.title) {
            tags.push("hymns");
        }
        add(x, tags);
    }
    for x in show("jt").map(Show::songs).unwrap_or_default() {
        let artist = x.artist.unwrap_or("");
        let mut tags = vec![
            "recital",
            if OWN.contains(&artist) {
                "originals"
            } else {
                "covers"
            },
        ];
        if ROCK.contains(&artist) {
            tags.push("rock");
        }
        add(x, tags);
    }
    out.sort_by(|a, b| a.song.title.cmp(b.song.title));
    out
});

/// The tags, most used first, with their counts.
pub static TAGS: LazyLock<Vec<(&'static str, usize)>> = LazyLock::new(|| {
    let mut tags: Vec<&'static str> = Vec::new();
    for x in LIB.iter() {
        for t in &x.tags {
            if !tags.contains(t) {
                tags.push(t);
            }
        }
    }
    let mut out: Vec<_> = tags
        .into_iter()
        .map(|t| (t, LIB.iter().filter(|x| x.tags.contains(&t)).count()))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
});

pub const TEMPI: [(&str, u32, u32); 4] = [
    ("Slow", 0, 80),
    ("Mid", 80, 110),
    ("Up", 110, 140),
    ("Fast", 140, 999),
];
pub const READY: [(char, &str); 4] = [
    ('c', "Chart"),
    ('l', "Lyrics"),
    ('s', "Stems"),
    ('m', "Tempo map"),
];

/// Weeks since a song was last played in a weekly set.
pub fn weeks_since(title: &str) -> f64 {
    OCC.iter()
        .filter(|o| o.date.from_today() < 0 && o.songs.iter().any(|x| x.title == title))
        .next_back()
        .map_or(f64::INFINITY, |o| -o.date.from_today() as f64 / 7.0)
}

/// A key's name as the circle of fifths spells it (`Gb` is `F#`), if it is one.
pub fn norm_key(k: &str) -> Option<&'static str> {
    let k = k.split('→').next().unwrap_or(k);
    FIFTHS.iter().copied().find(|f| *f == alias(k))
}

pub fn key_of(x: &Song) -> Option<&'static str> {
    x.key.map(|k| {
        let k = k.split('→').next().unwrap_or(k);
        FIFTHS.iter().copied().find(|f| *f == alias(k)).unwrap_or(k)
    })
}

pub fn cap(t: &str) -> String {
    let mut c = t.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

// ── Records ─────────────────────────────────────────────────────────────────

/// A track's status and a record's: where it is on the way out, and its hue.
pub const STAGES: [(&str, Option<u32>); 9] = [
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

pub struct Album {
    pub id: &'static str,
    pub title: &'static str,
    pub artist: &'static str,
    pub what: &'static str,
    pub status: usize,
    pub hue: u32,
    pub when: &'static str,
    pub tracks: &'static [(&'static str, usize)],
    pub owner: &'static str,
}

pub static ALBUMS: [Album; 4] = [
    Album {
        id: "cr",
        title: "Crescendum",
        artist: "Crescendum",
        what: "Album",
        status: 3,
        hue: 285,
        when: "2h",
        owner: "fasttrackstudio",
        tracks: &[
            ("Intro", 6),
            ("Set in Stone", 5),
            ("Unbreakable", 3),
            ("Chained Expectations", 3),
            ("Heavify", 2),
            ("It Knows My Name", 2),
            ("Kornesque", 1),
            ("The Ballad", 1),
            ("The Plague", 1),
            ("Empty Focus", 0),
        ],
    },
    Album {
        id: "png",
        title: "PNG Worship Collective Album",
        artist: "PNG Worship Collective",
        what: "Album",
        status: 2,
        hue: 150,
        when: "May 26",
        owner: "tombrooksmusic",
        tracks: &[
            ("All That I Am", 3),
            ("Lord of the Fight", 2),
            ("Dwelling Place", 2),
            ("Presence", 2),
            ("Life Giving Word", 1),
            ("He's Been Good", 2),
            ("I Know a Name", 2),
            ("Seek Your Face", 2),
            ("Hannah's Song", 2),
            ("Reason Why", 2),
            ("Goodness of God (RK version)", 3),
            ("Unity", 2),
        ],
    },
    Album {
        id: "mtl",
        title: "Montreal Benefit Album",
        artist: "Various artists",
        what: "Benefit album",
        status: 1,
        hue: 5,
        when: "Aug 4",
        owner: "tombrooksmusic",
        tracks: &[
            ("Courage", 3),
            ("Radio", 3),
            ("Too Young", 1),
            ("Where to Go", 2),
            ("You and I", 2),
        ],
    },
    Album {
        id: "joi",
        title: "A Journey of Immigrants",
        artist: "Andres Jaramillo",
        what: "Album",
        status: 5,
        hue: 45,
        when: "May 8",
        owner: "tombrooksmusic",
        tracks: &[
            ("Toccata", 5),
            ("Ancestro", 5),
            ("La Zacapaneca", 5),
            ("Five Colombian Pieces", 5),
            ("Luz Entre Aguas", 5),
            ("El Intachable", 5),
            ("Etudes for Immigrants", 5),
        ],
    },
];

pub fn album(id: &str) -> Option<&'static Album> {
    ALBUMS.iter().find(|a| a.id == id)
}

/// A song in production: a record's track, or a single.
pub struct Track {
    pub idx: usize,
    pub title: &'static str,
    pub rec: Option<&'static str>,
    pub when: &'static str,
    pub artist: &'static str,
    pub stage: usize,
    pub no: usize,
    pub hue: u32,
    pub cover: bool,
    pub owner: &'static str,
}

fn when_of(title: &str) -> Option<&'static str> {
    Some(match title {
        "Unbreakable" => "2h",
        "Chained Expectations" => "5h",
        "Heavify" | "The Ballad" => "Yesterday",
        "Intro" => "Mon",
        "Set in Stone" => "Last week",
        "It Knows My Name" => "Nov 10",
        "Kornesque" => "Nov 8",
        "The Plague" => "Nov 6",
        "Empty Focus" => "Oct 30",
        _ => return None,
    })
}

pub static STUDIO: LazyLock<Vec<Track>> = LazyLock::new(|| {
    let mut out = Vec::new();
    for a in ALBUMS.iter() {
        for (k, (title, stage)) in a.tracks.iter().enumerate() {
            out.push(Track {
                idx: out.len(),
                title,
                rec: Some(a.id),
                when: when_of(title).unwrap_or(a.when),
                artist: a.artist,
                stage: *stage,
                no: k + 1,
                hue: a.hue,
                cover: false,
                owner: a.owner,
            });
        }
    }
    let singles: [(&str, &str, usize, &str, u32, bool, &str); 4] = [
        (
            "Devour",
            "Beautifully Violent",
            3,
            "Jul 6",
            330,
            false,
            "fasttrackaudio",
        ),
        (
            "Goodness of God",
            "Ron Kenoly",
            4,
            "Oct 12",
            0,
            true,
            "tombrooksmusic",
        ),
        ("Fire", "JT Wright", 1, "Mon", 40, false, "codywright"),
        (
            "Drive To LA",
            "Nights Out",
            4,
            "Last week",
            10,
            false,
            "codywright",
        ),
    ];
    for (title, artist, stage, when, hue, cover, owner) in singles {
        out.push(Track {
            idx: out.len(),
            title,
            rec: None,
            when,
            artist,
            stage,
            no: 0,
            hue,
            cover,
            owner,
        });
    }
    out
});

pub fn track(title: &str) -> Option<&'static Track> {
    STUDIO.iter().find(|t| t.title == title)
}

impl Track {
    /// "t:Title", the id its art is kept under.
    pub fn key(&self) -> String {
        format!("t:{}", self.title)
    }
    pub fn art(&self) -> Option<Asset> {
        art(&self.key()).or_else(|| self.rec.and_then(art))
    }
    /// Its record's hue, or its own.
    pub fn cover_hue(&self) -> u32 {
        self.rec.and_then(album).map_or(self.hue, |a| a.hue)
    }
    pub fn cover_title(&self) -> &'static str {
        self.rec.and_then(album).map_or(self.title, |a| a.title)
    }
}

/// What Records shows, the last worked on first: records ("cr") and singles ("t:Fire").
pub const RECENT_WORK: [&str; 8] = [
    "cr",
    "t:Devour",
    "t:Fire",
    "t:Goodness of God",
    "t:Drive To LA",
    "mtl",
    "png",
    "joi",
];

pub fn owner_of(id: &str) -> &'static str {
    if let Some(a) = album(id) {
        return a.owner;
    }
    id.strip_prefix("t:")
        .and_then(track)
        .map_or("", |t| t.owner)
}

// ── Review ──────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct Comment {
    /// Where on the version, 0–1 (audio only).
    pub at: f64,
    pub t: String,
    pub who: &'static str,
    pub text: String,
    pub open: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Version {
    pub v: usize,
    pub what: &'static str,
    pub who: &'static str,
    pub when: String,
    pub len: u32,
    pub comments: Vec<Comment>,
    pub approved: bool,
}

fn c(at: f64, t: &str, who: &'static str, text: &str, open: bool) -> Comment {
    Comment {
        at,
        t: t.into(),
        who,
        text: text.into(),
        open,
    }
}

/// The audio reviews the vault already has: track, what, version, who, when, approved, comments.
fn known_audio(
    title: &str,
) -> Option<(
    &'static str,
    usize,
    &'static str,
    &'static str,
    bool,
    Vec<Comment>,
)> {
    Some(match title {
        "Unbreakable" => (
            "Mix",
            4,
            "Cody",
            "2h",
            false,
            vec![
                c(
                    0.38,
                    "1:42",
                    "Jake",
                    "Guitars eat the vocal in the pre-chorus",
                    true,
                ),
                c(
                    0.66,
                    "2:58",
                    "Mia",
                    "The delay throw here is perfect",
                    false,
                ),
                c(
                    0.81,
                    "3:37",
                    "Sam",
                    "Ride the bass up into the last chorus",
                    true,
                ),
            ],
        ),
        "Heavify" => (
            "Edit",
            2,
            "Cody",
            "Yesterday",
            false,
            vec![c(0.14, "0:37", "Sam", "Flam on the snare here", true)],
        ),
        "Intro" => (
            "Master",
            2,
            "Cody",
            "Mon",
            true,
            vec![c(0.5, "0:58", "Jake", "Approved", false)],
        ),
        _ => return None,
    })
}

/// Open comments on a track's chart and lyrics (they have no waveform).
pub fn other_open(title: &str) -> usize {
    match title {
        "Chained Expectations" => 1,
        "The Ballad" => 2,
        _ => 0,
    }
}

pub fn len_of(title: &str) -> u32 {
    170 + title.chars().map(|c| c as u32).sum::<u32>() % 160
}

/// A track's mix versions, newest last, each with its comments.
pub fn versions_for(t: &Track) -> Vec<Version> {
    let what = match t.stage {
        4.. => "Master",
        3 => "Mix",
        2 => "Edit",
        _ => "Rough",
    };
    let known = known_audio(t.title);
    let n = known.as_ref().map_or(t.stage.clamp(1, 4), |k| k.1);
    let len = len_of(t.title);
    let mut vs: Vec<Version> = (0..n)
        .map(|k| Version {
            v: k + 1,
            what: known.as_ref().map_or(what, |k| k.0),
            who: if k % 2 == 1 { "Jake" } else { "Cody" },
            when: if k == n - 1 {
                known.as_ref().map_or(t.when, |k| k.3).into()
            } else {
                format!("{} days ago", (n - 1 - k) * 4 + 3)
            },
            len,
            comments: Vec::new(),
            approved: false,
        })
        .collect();
    if let Some((_, _, who, _, approved, comments)) = known {
        let last = vs.last_mut().expect("a version");
        last.comments = comments;
        last.who = who;
        last.approved = approved;
    }
    if n > 1 {
        vs[n - 2].comments = vec![Comment {
            at: 0.22,
            t: dur(len as f64 * 0.22),
            who: "Mia",
            text: "Vocal a bit loud in verse 1".into(),
            open: false,
        }];
    }
    vs
}

pub fn all_versions() -> HashMap<&'static str, Vec<Version>> {
    STUDIO.iter().map(|t| (t.title, versions_for(t))).collect()
}

// ── Live: a set's chart and lyrics ──────────────────────────────────────────

/// The song's form: its sections and their lengths in bars (placeholders).
pub const SECS: [&str; 8] = [
    "Intro", "Verse 1", "Chorus", "Verse 2", "Chorus", "Bridge", "Chorus", "Outro",
];
pub const BARS: [usize; 8] = [4, 8, 8, 8, 8, 8, 8, 4];

pub fn sec_hue(n: &str) -> u32 {
    match n.split(' ').next().unwrap_or(n) {
        "Intro" | "Outro" => 250,
        "Verse" => 200,
        "Chorus" => 330,
        "Bridge" => 60,
        _ => 0,
    }
}

pub fn sec_short(n: &str) -> String {
    n.replace("Verse ", "V")
        .replace("Chorus", "C")
        .replace("Bridge", "B")
        .replace("Intro", "In")
        .replace("Outro", "Out")
}
