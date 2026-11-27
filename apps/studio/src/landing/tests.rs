//! The landing page's downloads, read from a release list as GitHub gives
//! it, and its room links.

use serde_json::{Value, json};

use super::*;

fn asset(name: &str) -> Value {
    json!({
        "name": name,
        "browser_download_url": format!("https://github.com/FastTrackStudios/studio/releases/download/x/{name}"),
    })
}

fn release(tag: &str, version: &str, prerelease: bool, body: &str) -> Value {
    json!({
        "tag_name": tag,
        "draft": false,
        "prerelease": prerelease,
        "html_url": format!("https://github.com/FastTrackStudios/studio/releases/tag/{tag}"),
        "body": body,
        "assets": [
            asset(&format!("Studio-{version}-macos-arm64.dmg")),
            asset(&format!("Studio-{version}-linux-x86_64.AppImage")),
            asset("SHA256SUMS"),
        ],
    })
}

fn url_of(list: &[Download], platform: Platform) -> &str {
    &list
        .iter()
        .find(|d| d.platform == platform)
        .expect("a button")
        .url
}

fn version_of_button(list: &[Download], platform: Platform) -> Option<&str> {
    list.iter()
        .find(|d| d.platform == platform)
        .and_then(|d| d.version.as_deref())
}

#[test]
fn every_platform_has_a_button_in_order() {
    let keys: Vec<&str> = downloads(&json!([]))
        .iter()
        .map(|d| d.platform.key())
        .collect();
    assert_eq!(keys, ["macos", "linux", "ios"]);
}

#[test]
fn before_the_list_is_read_every_button_goes_to_the_releases_page() {
    for d in unread() {
        assert_eq!(d.url, RELEASES_PAGE);
        assert_eq!(d.version, None);
    }
}

#[test]
fn the_desktop_buttons_download_the_newest_releases_files() {
    let list = json!([
        release("v26.11.5", "26.11.5", false, ""),
        release("studio-v26.12.1", "26.12.1", true, ""),
        release("studio-v26.11.9", "26.11.9", true, ""),
    ]);
    let got = downloads(&list);
    assert!(url_of(&got, Platform::Macos).ends_with("/Studio-26.12.1-macos-arm64.dmg"));
    assert!(url_of(&got, Platform::Linux).ends_with("/Studio-26.12.1-linux-x86_64.AppImage"));
    assert_eq!(version_of_button(&got, Platform::Macos), Some("26.12.1"));
}

#[test]
fn a_pre_release_counts_studios_alpha_is_one() {
    let list = json!([
        release("studio-v26.11.6", "26.11.6", true, ""),
        release("v26.11.5", "26.11.5", false, ""),
    ]);
    assert_eq!(
        version_of_button(&downloads(&list), Platform::Linux),
        Some("26.11.6")
    );
}

#[test]
fn versions_compare_as_numbers_not_text() {
    let list = json!([
        release("studio-v26.9.3", "26.9.3", true, ""),
        release("studio-v26.11.2", "26.11.2", true, ""),
    ]);
    assert_eq!(
        version_of_button(&downloads(&list), Platform::Macos),
        Some("26.11.2")
    );
}

#[test]
fn a_draft_or_a_tag_that_is_not_a_release_is_skipped() {
    let mut draft = release("studio-v27.1.1", "27.1.1", true, "");
    draft["draft"] = json!(true);
    let list = json!([
        draft,
        release("setlists-v0.1.0", "0.1.0", false, ""),
        release("studio-v26.11.6", "26.11.6", true, ""),
    ]);
    assert_eq!(
        version_of_button(&downloads(&list), Platform::Macos),
        Some("26.11.6")
    );
}

#[test]
fn a_release_without_a_platforms_file_falls_back_to_an_older_one_that_has_it() {
    // The newest release's Linux build failed: its macOS file only.
    let mut newest = release("studio-v26.12.1", "26.12.1", true, "");
    newest["assets"] = json!([asset("Studio-26.12.1-macos-arm64.dmg")]);
    let list = json!([newest, release("studio-v26.11.6", "26.11.6", true, "")]);
    let got = downloads(&list);
    assert_eq!(version_of_button(&got, Platform::Macos), Some("26.12.1"));
    assert_eq!(version_of_button(&got, Platform::Linux), Some("26.11.6"));
}

#[test]
fn no_release_with_the_file_is_the_releases_page() {
    let mut bare = release("studio-v26.12.1", "26.12.1", true, "");
    bare["assets"] = json!([]);
    let got = downloads(&json!([bare]));
    assert_eq!(url_of(&got, Platform::Macos), RELEASES_PAGE);
    assert_eq!(version_of_button(&got, Platform::Macos), None);
}

#[test]
fn iphone_and_ipad_go_to_the_testflight_link_in_the_newest_notes() {
    let list = json!([
        release(
            "studio-v26.12.1",
            "26.12.1",
            true,
            "On iPhone and iPad: https://testflight.apple.com/join/AbC123xy (TestFlight).",
        ),
        release(
            "studio-v26.11.6",
            "26.11.6",
            true,
            "https://testflight.apple.com/join/OLDOLD",
        ),
    ]);
    let got = downloads(&list);
    assert_eq!(
        url_of(&got, Platform::Ios),
        "https://testflight.apple.com/join/AbC123xy"
    );
    assert_eq!(version_of_button(&got, Platform::Ios), Some("26.12.1"));
}

#[test]
fn with_no_testflight_link_iphone_and_ipad_go_to_the_newest_releases_page() {
    let list = json!([
        release("studio-v26.11.6", "26.11.6", true, ""),
        release("studio-v26.12.1", "26.12.1", true, "No link here."),
    ]);
    assert_eq!(
        url_of(&downloads(&list), Platform::Ios),
        "https://github.com/FastTrackStudios/studio/releases/tag/studio-v26.12.1"
    );
}

#[test]
fn an_answer_that_is_not_a_list_is_the_releases_page() {
    // GitHub's rate limit answers with an object.
    let got = downloads(&json!({ "message": "API rate limit exceeded" }));
    assert_eq!(got, unread());
}

// ── rooms ───────────────────────────────────────────────────────────────

#[test]
fn a_room_is_found_in_every_form_of_its_link() {
    for typed in [
        "https://fasttrackstudio.app/join/adult-jam",
        "http://fasttrackstudio.app/join/adult-jam/",
        "fasttrackstudio.app/join/adult-jam",
        "fasttrackstudio://join/adult-jam",
        "  adult-jam  ",
    ] {
        assert_eq!(room_in(typed).as_deref(), Some("adult-jam"), "{typed}");
    }
}

#[test]
fn what_is_not_a_rooms_link_is_refused() {
    for typed in [
        "",
        "   ",
        "https://example.com/join/adult-jam",
        "fasttrackstudio.app/join/",
        "Adult Jam",
        "adult_jam",
        "fasttrackstudio.app/join/a/b",
    ] {
        assert_eq!(room_in(typed), None, "{typed:?}");
    }
}

#[test]
fn the_room_link_flow_is_the_rooms_link_on_this_site() {
    assert_eq!(join_path("adult-jam", ""), "/join/adult-jam");
    assert_eq!(join_path("adult-jam", "/studio/"), "/studio/join/adult-jam");
}

#[test]
fn a_rooms_link_opened_on_this_site_names_the_room() {
    assert_eq!(
        room_on_path("/join/adult-jam", "").as_deref(),
        Some("adult-jam")
    );
    assert_eq!(
        room_on_path("/studio/join/adult-jam/", "studio").as_deref(),
        Some("adult-jam")
    );
    assert_eq!(room_on_path("/", ""), None);
    assert_eq!(room_on_path("/setlists", ""), None);
    assert_eq!(room_on_path("/join/Not A Room", ""), None);
    // Under another base, the root's room path is not this page's.
    assert_eq!(room_on_path("/join/adult-jam", "studio"), None);
}

#[test]
fn every_path_join_path_makes_is_read_back_by_room_on_path() {
    for base in ["", "studio", "/beta/"] {
        let path = join_path("crossroads-jam", base);
        assert_eq!(
            room_on_path(&path, base).as_deref(),
            Some("crossroads-jam"),
            "{path} under {base:?}"
        );
    }
}

#[test]
fn the_apps_link_opens_studio_in_the_room() {
    assert_eq!(app_link("adult-jam"), "fasttrackstudio://join/adult-jam");
    assert_eq!(
        room_in(&app_link("adult-jam")).as_deref(),
        Some("adult-jam")
    );
}
