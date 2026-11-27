use super::*;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn a_link_given_at_launch() {
    let link = "https://fasttrackstudio.app/join/adult-jam-3f9a1c";
    assert_eq!(
        link_in(&args(&["Studio", "--join", link])).as_deref(),
        Some(link)
    );
    assert_eq!(
        link_in(&args(&["Studio", &format!("--join={link}")])).as_deref(),
        Some(link)
    );
    assert_eq!(
        link_in(&args(&["Studio", "--crash-test", "--join", link])).as_deref(),
        Some(link)
    );
    // The program's own name is never the link, nor an empty one.
    assert_eq!(link_in(&args(&["--join"])), None);
    assert_eq!(link_in(&args(&["Studio", "--join"])), None);
    assert_eq!(link_in(&args(&["Studio", "--join", " "])), None);
    assert_eq!(link_in(&args(&["Studio", "--version"])), None);
}

#[test]
fn a_rooms_link_in_either_scheme_and_nothing_else() {
    assert!(is_room_link(
        "https://fasttrackstudio.app/join/adult-jam-3f9a1c"
    ));
    assert!(is_room_link("fasttrackstudio://join/adult-jam-3f9a1c"));
    assert!(!is_room_link("fasttrackstudio://set/adult-jam-3f9a1c"));
    assert!(!is_room_link("https://example.com/join/adult-jam"));
}

#[test]
fn rooms_are_kept_in_the_apps_studio_folder() {
    let home = Path::new("/data/Library/Application Support/fasttrackstudio/Studio");
    assert_eq!(rooms_dir(Some(home)), Some(home.join("Rooms")));
    assert_eq!(rooms_dir(None), None);
}
