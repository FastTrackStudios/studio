use super::*;

fn track(i: u32, name: &str) -> Track {
    Track::new(format!("guid-{i}"), i, name.to_string())
}

fn tracks(names: &[&str]) -> Vec<Track> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| track(u32::try_from(i).unwrap_or(0), n))
        .collect()
}

/// The tree as text: `Folder[child, child]`, a bare name for a strip.
fn shape(nodes: &[MixNode]) -> Vec<String> {
    nodes
        .iter()
        .map(|n| match n {
            MixNode::Leaf(t) => t.name.clone(),
            MixNode::Folder(f) => format!("{}[{}]", f.name, shape(&f.children).join(", ")),
        })
        .collect()
}

#[test]
fn a_lone_reference_or_loop_gets_its_own_folder_and_reference_goes_first() {
    let [kick, snare, loop_, original] = [
        track(0, "Kick"),
        track(1, "Snare"),
        track(2, "Loop"),
        track(3, "Original Track"),
    ];
    let sorted = vec![
        MixNode::Folder(FolderNode {
            name: "Drums".into(),
            children: vec![MixNode::Leaf(kick), MixNode::Leaf(snare)],
        }),
        MixNode::Leaf(loop_),
        MixNode::Leaf(original),
    ];
    assert_eq!(
        shape(&with_homes(sorted)),
        [
            "Reference[Original Track]",
            "Drums[Kick, Snare]",
            "Tracks[Loop]"
        ],
    );
}

#[test]
fn a_reference_the_sorter_already_foldered_moves_left_and_keeps_its_strips() {
    let sorted = vec![
        MixNode::Leaf(track(0, "Bass")),
        MixNode::Folder(FolderNode {
            name: "Reference".into(),
            children: vec![MixNode::Leaf(track(1, "Reference Mix"))],
        }),
        MixNode::Leaf(track(2, "Backing Vocals Reference")),
    ];
    assert_eq!(
        shape(&with_homes(sorted)),
        ["Reference[Reference Mix, Backing Vocals Reference]", "Bass"],
    );
}

#[test]
fn the_native_mixer_opens_on_the_reference() {
    let tree = organize_dynamic(&tracks(&["Kick", "Snare", "Bass", "Original Track"]));
    let shape = shape(&tree);
    assert_eq!(
        shape.first().map(String::as_str),
        Some("Reference[Original Track]"),
        "{shape:?}",
    );
}
