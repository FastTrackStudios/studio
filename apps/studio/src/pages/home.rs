//! Home: the real work of the org Studio is in (the one the sidebar's
//! switcher picked): its setlists, its records and the songs changed
//! most recently. Each says how to add the real thing when there is none
//! yet. Home lists no orgs: switching is the switcher's.

use dioxus::prelude::*;

use super::{FaceRow, progress_of};
use crate::Route;
use crate::day::{DAY, MONTH, today};
use crate::files::Hit;
use crate::icons::{I, Icon};
use crate::pieces::{Bar, Page};
use crate::records::Record;
use crate::setlist::Face;
use crate::state::use_app;

/// What Home shows of an org: its sets (the index's, soonest first, then
/// undated), its setlist files and its records. Pure, for the tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrgWork {
    pub sets: Vec<Face>,
    pub files: Vec<Hit>,
    pub records: Vec<Record>,
}

impl OrgWork {
    /// The work of `org`, from what Studio has found.
    pub fn of(org: &str, faces: &[Face], files: &[Hit], records: &[Record]) -> OrgWork {
        let mut sets: Vec<Face> = faces
            .iter()
            .filter(|f| f.kind != crate::setlist::Kind::Template)
            .cloned()
            .collect();
        sets.sort_by(|a, b| match (a.upcoming(), b.upcoming()) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.date.cmp(&b.date),
        });
        let mut files: Vec<Hit> = files.iter().filter(|h| h.space == org).cloned().collect();
        // A set of the index's that has its own file is listed once.
        files.retain(|h| {
            let name = crate::setfile::name_of(&h.name);
            !sets.iter().any(|f| f.title == name)
        });
        OrgWork {
            sets,
            files,
            records: super::records::of_org(records, Some(org)),
        }
    }

    pub fn setlists(&self) -> usize {
        self.sets.len() + self.files.len()
    }
}

/// A song folder in Studio's home (`Songs/<Title - Artist>/`), as Home
/// lists it: most recently changed first.
#[derive(Clone, Debug, PartialEq)]
pub struct RecentSong {
    pub title: String,
    pub artist: Option<String>,
    /// The folder's path on this computer.
    pub folder: String,
    /// When it last changed, in ms since 1970.
    pub mtime: i64,
}

/// The `limit` song folders in `<home>/Songs/` changed most recently.
pub fn recent_songs(limit: usize) -> Vec<RecentSong> {
    if cfg!(target_arch = "wasm32") {
        return Vec::new();
    }
    let mut v: Vec<RecentSong> = crate::files_live::disk::list("", "Songs")
        .into_iter()
        .filter(|h| h.dir && !h.name.starts_with('.'))
        .map(|h| {
            let (title, artist) = match h.name.split_once(" - ") {
                Some((t, a)) => (t.trim().to_string(), Some(a.trim().to_string())),
                None => (h.name.clone(), None),
            };
            RecentSong {
                title,
                artist,
                folder: crate::storage::resolve(&h.path)
                    .map_or(h.path.clone(), |p| p.to_string_lossy().into_owned()),
                mtime: h.mtime,
            }
        })
        .collect();
    v.sort_by(|a, b| b.mtime.cmp(&a.mtime).then(a.title.cmp(&b.title)));
    v.truncate(limit);
    v
}

#[component]
pub fn Home() -> Element {
    let app = use_app();
    let index = crate::sets::use_index();
    let here = (app.here)();
    let org = (app.org)().unwrap_or_else(|| here.clone());
    // The index is the org Studio is in's; another org's sets are its own.
    let faces = if org == here {
        index.faces()
    } else {
        Vec::new()
    };
    let files = (app.drive_sets)();
    let records = (app.records)();
    let work = OrgWork::of(
        &org,
        &faces,
        &files.clone().unwrap_or_default(),
        &records.clone().unwrap_or_default(),
    );
    // Read off the UI thread: Drive's mount can stall.
    let recent = use_resource(|| crate::state::off_thread(|| recent_songs(6)));
    let recent = move || recent().flatten();
    let name = crate::orgs::org_name(&org);
    let t = today();
    let date = format!("{}, {} {}", DAY[t.weekday()], MONTH[t.month0()], t.mday());
    let no_org = cfg!(feature = "drive") && (app.orgs)().is_empty();

    rsx! {
        Page {
            Bar { title: "Home" }
            div { class: "body",
                div { class: "hdate", "{date}" }
                if no_org {
                    div { class: "dayhead", "Welcome to Studio" }
                    p { class: "lede",
                        "You are not in an organization yet. Make one for your band, church or school, or ask its owner to invite you."
                    }
                    NewOrgHere {}
                } else {
                    div { class: "dayhead dh2",
                        span { "Setlists" }
                        button {
                            class: "seeall",
                            onclick: move |_| {
                                navigator().push(Route::Setlists {});
                            },
                            "All "
                            Icon { name: I::ChevRight, size: 14 }
                        }
                    }
                    if files.is_none() && work.setlists() == 0 {
                        p { class: "empty", "Looking for {name}'s setlists…" }
                    } else if work.setlists() == 0 {
                        div { class: "empty", role: "status",
                            p { "{name} has no setlists yet." }
                            p {
                                "Put a "
                                code { ".setlist" }
                                " file in Studio's home under "
                                code { "Setlists/" }
                                ", or make one on the Setlists page."
                            }
                        }
                    } else {
                        div { class: "agenda",
                            for f in work.sets.clone() {
                                FaceRow { key: "{f.id}", face: f.clone(), owner: org.clone(), state: progress_of(&f) }
                            }
                            for h in work.files.clone() {
                                FileRow { key: "{h.location}/{h.path}", file: h }
                            }
                        }
                    }
                    div { class: "dayhead dh2",
                        span { "Records" }
                        button {
                            class: "seeall",
                            onclick: move |_| {
                                navigator().push(Route::Records {});
                            },
                            "All "
                            Icon { name: I::ChevRight, size: 14 }
                        }
                    }
                    if records.is_none() {
                        p { class: "empty", "Looking for records…" }
                    } else if work.records.is_empty() {
                        super::records::NoRecords {}
                    } else {
                        div { class: "lgrid2",
                            for r in work.records.clone() {
                                super::records::RecordTile { key: "{r.id}", record: r }
                            }
                        }
                    }
                    if !cfg!(target_arch = "wasm32") {
                        div { class: "dayhead", "Recent songs" }
                        if recent().is_none() {
                            p { class: "empty", "Looking for songs…" }
                        } else if recent().unwrap_or_default().is_empty() {
                            div { class: "empty", role: "status",
                                p { "No songs in Studio's home yet." }
                                p {
                                    "Each song is a folder, "
                                    code { "Songs/<Title - Artist>/" }
                                    ", holding its "
                                    code { ".session" }
                                    "."
                                }
                            }
                        } else {
                            div { class: "agenda",
                                for s in recent().unwrap_or_default() {
                                    RecentRow { key: "{s.folder}", song: s }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Make the first org, when Drive says this account is in none.
#[component]
fn NewOrgHere() -> Element {
    #[cfg(feature = "drive")]
    return rsx! { crate::pages::orgs::NewOrg {} };
    #[cfg(not(feature = "drive"))]
    rsx! {}
}

/// A setlist file, opening where it is kept.
#[component]
fn FileRow(file: Hit) -> Element {
    let title = crate::setfile::name_of(&file.name);
    let to = Route::DriveSet {
        space: file.space.clone(),
        location: file.location.clone(),
        path: file.path.split('/').map(str::to_string).collect(),
    };
    rsx! {
        button {
            class: "ag",
            onclick: move |_| {
                navigator().push(to.clone());
            },
            span { class: "cal tpl",
                Icon { name: I::Setlist, size: 18 }
            }
            span { class: "mid",
                span { class: "t1", "{title}" }
                span { class: "s1", "Setlist file" }
            }
            span { class: "chev",
                Icon { name: I::ChevRight, size: 16 }
            }
        }
    }
}

/// A song folder changed recently: it opens as a session of its own.
#[component]
fn RecentRow(song: RecentSong) -> Element {
    let app = use_app();
    let when = crate::day::Day((song.mtime / 86_400_000) as i32).when();
    let line = match &song.artist {
        Some(a) => format!("{a} · changed {}", when.to_lowercase()),
        None => format!("Changed {}", when.to_lowercase()),
    };
    let hue = crate::look::str_hue(song.artist.as_deref().unwrap_or(&song.title));
    rsx! {
        button {
            class: "ag",
            onclick: move |_| {
                let place = crate::state::Place {
                    space: (app.here)(),
                    location: crate::files::DISK.into(),
                    folder: song.folder.clone(),
                    chart: None,
                    arrangement: None,
                };
                crate::session::enter(
                    app,
                    vec![(crate::song::song_named(&song.title), place)],
                    song.title.clone(),
                    0,
                    false,
                    Some(Route::Home {}),
                    crate::session::View::Arrangement,
                );
            },
            span { class: "aglogo",
                crate::pieces::Cover { title: song.title.clone(), size: 36, hue }
            }
            span { class: "mid",
                span { class: "t1", "{song.title}" }
                span { class: "s1", "{line}" }
            }
            span { class: "chev",
                Icon { name: I::ChevRight, size: 16 }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(space: &str, name: &str) -> Hit {
        Hit {
            space: space.into(),
            location: crate::files::DISK.into(),
            path: format!("Setlists/{name}"),
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn home_shows_only_the_org_s_own_work() {
        let files = [
            file("codywright", "JT's Senior Recital.setlist"),
            file("crossroads-church", "Days to Praise.setlist"),
        ];
        let w = OrgWork::of("codywright", &[], &files, &[]);
        assert_eq!(w.files.len(), 1);
        assert_eq!(w.files[0].name, "JT's Senior Recital.setlist");
        let w = OrgWork::of("crossroads-church", &[], &files, &[]);
        assert_eq!(w.files[0].name, "Days to Praise.setlist");
    }

    #[test]
    fn an_empty_home_has_nothing_made_up() {
        let w = OrgWork::of("adult-jam", &[], &[], &[]);
        assert_eq!(w, OrgWork::default());
        assert_eq!(w.setlists(), 0);
    }
}
