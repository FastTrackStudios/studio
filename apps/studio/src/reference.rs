//! Finding a song's reference: the mix its session bounced
//! (`<song>/Media/Proxies/Reference.ogg`, what Session's Reference mode
//! plays), on whichever computer has the song's session folder.
//!
//! A session folder is named by its song's slug (`know-me`,
//! `7-nation-army`), which need not be the library's title ("Know Me
//! (Extended)", "Seven Nation Army"): a folder is the song's when every
//! word of its name is a word of the title, numbers and their words alike.

use crate::files::Hit;

/// Where a session keeps its reference, under its folder.
pub const PROXY: &str = "Media/Proxies/Reference.ogg";

/// The session folder (its path) of a reference proxy's path.
pub fn folder_of(path: &str) -> Option<&str> {
    let folder = path.strip_suffix(PROXY)?;
    Some(folder.strip_suffix('/').unwrap_or(folder))
}

/// The words of a title or a folder name, lower-case, numbers as digits.
fn words(s: &str) -> Vec<String> {
    session_sync::slug::slugify(s)
        .split('-')
        .filter(|w| !w.is_empty())
        .map(|w| match w {
            "one" => "1",
            "two" => "2",
            "three" => "3",
            "four" => "4",
            "five" => "5",
            "six" => "6",
            "seven" => "7",
            "eight" => "8",
            "nine" => "9",
            "ten" => "10",
            w => w,
        })
        .map(str::to_string)
        .collect()
}

/// How well a session folder's name fits a title: the number of its
/// words, when every one is in the title.
pub fn fit(title: &str, folder: &str) -> Option<usize> {
    let name = folder.rsplit('/').next().unwrap_or(folder);
    let (t, f) = (words(title), words(name));
    (!f.is_empty() && f.iter().all(|w| t.contains(w))).then_some(f.len())
}

/// The song's reference among every reference found: the best-fitting
/// folder, on a computer that is on, the newest bounce first.
pub fn pick<'a>(title: &str, hits: &'a [Hit]) -> Option<&'a Hit> {
    hits.iter()
        .filter_map(|h| Some((fit(title, folder_of(&h.path)?)?, h)))
        .max_by(|(fa, a), (fb, b)| {
            fa.cmp(fb)
                .then(a.online.cmp(&b.online))
                .then(a.mtime.cmp(&b.mtime))
        })
        .map(|(_, h)| h)
}

/// A song's chart among every `.kf` found, for its session folder: one
/// whose name has every word of the folder's and the fewest others
/// ("Fire" over "Kings of Leon - Sex on Fire"; a track number is not a
/// word), never an archived draft, the newest first.
pub fn pick_chart<'a>(folder: &str, charts: &'a [Hit]) -> Option<&'a Hit> {
    let name = folder.rsplit('/').next().unwrap_or(folder);
    let want = words(name);
    charts
        .iter()
        .filter(|h| !h.path.contains("/Archive/") && h.name.ends_with(".kf"))
        .filter_map(|h| {
            let stem = h.name.trim_end_matches(".kf");
            let fit = fit(stem, name)?;
            let extra = words(stem)
                .iter()
                .filter(|w| !want.contains(w) && !w.chars().all(|c| c.is_ascii_digit()))
                .count();
            Some(((fit, std::cmp::Reverse(extra)), h))
        })
        .max_by(|(ka, a), (kb, b)| {
            ka.cmp(kb)
                .then(a.online.cmp(&b.online))
                .then(a.mtime.cmp(&b.mtime))
        })
        .map(|(_, h)| h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(path: &str, online: bool, mtime: i64) -> Hit {
        Hit {
            path: path.into(),
            online,
            mtime,
            ..Default::default()
        }
    }

    #[test]
    fn a_folder_fits_its_titles() {
        assert_eq!(fit("Know Me (Extended)", "know-me"), Some(2));
        assert_eq!(fit("Seven Nation Army", "7-nation-army"), Some(3));
        assert_eq!(fit("DTLA Cinematic Intro", "cinematic-intro"), Some(2));
        assert_eq!(fit("Heart Isn't Ready", "heart-isnt-ready"), Some(3));
        assert_eq!(fit("Hey!", "hey"), Some(1));
        assert_eq!(fit("Fire", "for-you"), None);
        assert_eq!(fit("Fire", "fasttrackaudio/session/fire"), Some(1));
    }

    #[test]
    fn a_songs_chart_is_the_closest_name() {
        let kf = |path: &str, mtime: i64| Hit {
            name: path.rsplit('/').next().unwrap().into(),
            path: path.into(),
            online: true,
            mtime,
            ..Default::default()
        };
        let charts = vec![
            kf("Chordsheet Backup/kf/KINGS OF LEON - SEX ON FIRE.kf", 9),
            kf("Projects/JT's Senior Recital/charts/08 Fire.kf", 1),
            kf(
                "Projects/JT's Senior Recital/charts/10 Know Me (Extended).kf",
                1,
            ),
            kf("Libraries/Christmas/Sleigh Ride/Sleigh Ride.kf", 1),
            kf(
                "Libraries/Christmas/Sleigh Ride/Archive/Sleigh Ride (draft).kf",
                9,
            ),
        ];
        let got = |f: &str| pick_chart(f, &charts).map(|h| h.name.as_str());
        assert_eq!(got("fire"), Some("08 Fire.kf"));
        assert_eq!(got("know-me"), Some("10 Know Me (Extended).kf"));
        assert_eq!(got("songs/sleigh-ride"), Some("Sleigh Ride.kf"));
        assert_eq!(got("sleeptalker"), None);
    }

    #[test]
    fn the_best_folder_on_a_computer_that_is_on_wins() {
        let hits = vec![
            hit("know-me/Media/Proxies/Reference.ogg", true, 1),
            hit("fta/session/know-me/Media/Proxies/Reference.ogg", true, 5),
            hit("me/Media/Proxies/Reference.ogg", true, 9),
            hit("x/know-me/Media/Proxies/Reference.ogg", false, 9),
            hit("know-me/Media/Proxies/Reference Preview.ogg", true, 9),
        ];
        let got = pick("Know Me (Extended)", &hits).unwrap();
        assert_eq!(got.path, "fta/session/know-me/Media/Proxies/Reference.ogg");
        assert_eq!(folder_of(&got.path), Some("fta/session/know-me"));
        assert_eq!(folder_of(PROXY), Some(""));
    }
}
