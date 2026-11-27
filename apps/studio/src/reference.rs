//! Finding a song's reference: the mix its session bounced, as a proxy
//! (`<song>/Media/Proxy/Reference/<x>.ogg`, the proxy of
//! `Media/Reference/<x>.wav`; before the layout, `Media/Proxies/Reference.ogg`,
//! still read), on whichever computer has the song's session folder.
//!
//! A session folder is named by its song's slug (`know-me`,
//! `7-nation-army`), which need not be the library's title ("Know Me
//! (Extended)", "Seven Nation Army"): a folder is the song's when every
//! word of its name is a word of the title, numbers and their words alike.

use crate::files::Hit;

/// Where a session kept its reference before the layout, under its folder.
pub const PROXY: &str = "Media/Proxies/Reference.ogg";

/// The folder a song's reference proxies are in, under its folder: each is
/// `<x>.ogg`, the proxy of `Media/Reference/<x>.wav`.
pub const PROXY_DIR: &str = "Media/Proxy/Reference";

/// The session folder (its path) of a reference proxy's path, in either
/// layout.
pub fn folder_of(path: &str) -> Option<&str> {
    let folder = match path.strip_suffix(PROXY) {
        Some(folder) => folder,
        None => {
            let (dir, name) = path.rsplit_once('/')?;
            let stem = name.strip_suffix(".ogg")?;
            // A reference's preview is not the reference.
            if stem.ends_with(" Preview") {
                return None;
            }
            dir.strip_suffix(PROXY_DIR)?
        }
    };
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

/// The folder an arrangement's own files are in, under its song's folder
/// (`Arrangements/<name>`, as the chart extension's `song_folder` reads it).
pub fn arrangement_dir(folder: &str, arrangement: &str) -> String {
    format!("{folder}/Arrangements/{arrangement}")
}

/// The song's session file in its folder's listing: the old pair's
/// `.fts-session` while the folder has one, else the song's one
/// `.session` (`crate::session_file` tells the two apart by their bytes).
pub fn session_of(listing: &[Hit]) -> Option<&Hit> {
    let file = |ext: &str| listing.iter().find(|h| !h.dir && h.name.ends_with(ext));
    file(".fts-session").or_else(|| file(".session"))
}

/// The chart a set's place follows.
///
/// The song as recorded follows the chart its session keeps (`session`:
/// its session file, when that holds one, `crate::session_file::chart`),
/// where each song's chart moved from its `.kf`. A song not moved yet
/// follows its `.kf`: one in `own` (its folder's listing), else one named
/// like its folder, else like its `title`. With neither, the chart
/// generated from its session shows.
///
/// A place that plays an arrangement follows that arrangement's: a `.kf`
/// in its folder ([`arrangement_dir`]; `arranged` is that folder's
/// listing, else one Drive found there among `charts`), else one named
/// like the song and the arrangement ("Know Me (Extended)"). Never the
/// song's own, which is another form, often in another key (so never the
/// chart the song's session keeps on its `main` branch either): with
/// none, the chart generated from the arrangement's session shows.
pub fn place_chart(
    folder: &str,
    title: &str,
    arrangement: Option<&str>,
    session: Option<&Hit>,
    own: &[Hit],
    arranged: &[Hit],
    charts: &[Hit],
) -> Option<Hit> {
    let kf = |h: &&Hit| !h.dir && h.name.ends_with(".kf");
    let name = folder.rsplit('/').next().filter(|n| !n.is_empty());
    let Some(arrangement) = arrangement else {
        return session
            .or_else(|| own.iter().find(kf))
            .or_else(|| name.and_then(|n| pick_chart(n, charts)))
            .or_else(|| pick_chart(title, charts))
            .cloned();
    };
    let dir = (!folder.is_empty()).then(|| format!("{}/", arrangement_dir(folder, arrangement)));
    let in_dir = || {
        let dir = dir.as_deref()?;
        charts
            .iter()
            .filter(kf)
            .filter(|h| h.path.starts_with(dir) && !h.path[dir.len()..].contains('/'))
            .min_by(|a, b| a.name.cmp(&b.name))
    };
    let named = |song: &str| pick_chart(&format!("{song} {arrangement}"), charts);
    arranged
        .iter()
        .filter(kf)
        .min_by(|a, b| a.name.cmp(&b.name))
        .or_else(in_dir)
        .or_else(|| name.and_then(named))
        .or_else(|| named(title))
        .cloned()
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

    /// #624: a song whose chart moved into its session follows that one,
    /// with no `.kf` anywhere; a place that plays an arrangement still
    /// follows the arrangement's own, never the song's.
    #[test]
    fn a_place_follows_the_chart_its_session_keeps() {
        let file = |path: &str| Hit {
            name: path.rsplit('/').next().unwrap().into(),
            path: path.into(),
            online: true,
            ..Default::default()
        };
        let folder = "Studio/Songs/Fire - JT Wright";
        let listing = vec![
            file("Studio/Songs/Fire - JT Wright/Fire.session"),
            file("Studio/Songs/Fire - JT Wright/Fire.session.log"),
            Hit {
                dir: true,
                ..file("Studio/Songs/Fire - JT Wright/Media")
            },
        ];
        let session = session_of(&listing);
        assert_eq!(
            session.map(|h| h.name.as_str()),
            Some("Fire.session"),
            "the song's one file, not its log"
        );
        // A `.kf` of the same name elsewhere is not the song's chart now.
        let charts = vec![file("Projects/JT's Senior Recital/charts/08 Fire.kf")];
        let got = |arr: Option<&str>| {
            place_chart(folder, "Fire", arr, session, &listing, &[], &charts).map(|h| h.path)
        };
        assert_eq!(
            got(None).as_deref(),
            Some("Studio/Songs/Fire - JT Wright/Fire.session")
        );
        // An arrangement with no chart of its own: none (its generated
        // chart shows), not the song's.
        assert_eq!(got(Some("Acoustic")), None);
        // The old pair's document is the session while a folder has one.
        let pair = vec![
            file("Songs/Intro/Intro.session"),
            file("Songs/Intro/Intro.fts-session"),
        ];
        assert_eq!(
            session_of(&pair).map(|h| h.name.as_str()),
            Some("Intro.fts-session")
        );
    }

    /// #624: a set's place that plays an arrangement follows that
    /// arrangement's chart, never the song's own (another form, often
    /// another key). A song whose session keeps no chart (one not moved
    /// from its `.kf` yet) follows its `.kf`: the fallback.
    #[test]
    fn a_place_that_plays_an_arrangement_follows_its_chart() {
        let kf = |path: &str| Hit {
            name: path.rsplit('/').next().unwrap().into(),
            path: path.into(),
            online: true,
            ..Default::default()
        };
        let folder = "Libraries/L/Know Me";
        // The song's own chart (in G), beside its session.
        let own = vec![kf("Libraries/L/Know Me/Know Me.kf")];
        // Its Big Intro arrangement's (in A), in its arrangement's folder.
        let big_intro = vec![kf("Libraries/L/Know Me/Arrangements/Big Intro/Know Me.kf")];
        let charts = vec![
            own[0].clone(),
            big_intro[0].clone(),
            kf("Libraries/L/Know Me/Arrangements/Extended/Know Me (Extended).kf"),
            kf("Projects/Recital/charts/10 Know Me (Acoustic).kf"),
        ];
        let got = |arr: Option<&str>, arranged: &[Hit]| {
            place_chart(folder, "Know Me", arr, None, &own, arranged, &charts).map(|h| h.path)
        };
        // The song as recorded: its own chart.
        assert_eq!(
            got(None, &[]).as_deref(),
            Some("Libraries/L/Know Me/Know Me.kf")
        );
        // Big Intro: its folder's chart, listed.
        assert_eq!(
            got(Some("Big Intro"), &big_intro).as_deref(),
            Some("Libraries/L/Know Me/Arrangements/Big Intro/Know Me.kf")
        );
        // Extended: its folder's chart, as Drive found it (no listing).
        assert_eq!(
            got(Some("Extended"), &[]).as_deref(),
            Some("Libraries/L/Know Me/Arrangements/Extended/Know Me (Extended).kf")
        );
        // Acoustic: a chart named like the song and the arrangement.
        assert_eq!(
            got(Some("Acoustic"), &[]).as_deref(),
            Some("Projects/Recital/charts/10 Know Me (Acoustic).kf")
        );
        // Shortened has none: no chart (the one generated from the
        // arrangement's session shows), never the song's own.
        assert_eq!(got(Some("Shortened"), &[]), None);
        // A song with no session found: by its title and the arrangement.
        assert_eq!(
            place_chart("", "Know Me", Some("Acoustic"), None, &[], &[], &charts)
                .map(|h| h.path)
                .as_deref(),
            Some("Projects/Recital/charts/10 Know Me (Acoustic).kf")
        );
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
        // The new layout's, and its preview not taken for it.
        assert_eq!(
            folder_of("Songs/Know Me - JT/Media/Proxy/Reference/Know Me.ogg"),
            Some("Songs/Know Me - JT")
        );
        assert_eq!(
            folder_of("Songs/Know Me - JT/Media/Proxy/Reference/Know Me Preview.ogg"),
            None
        );
        assert_eq!(folder_of("Songs/Know Me - JT/Media/Proxy/Bass.ogg"), None);
        assert_eq!(folder_of(&got.path), Some("fta/session/know-me"));
        assert_eq!(folder_of(PROXY), Some(""));
    }
}
