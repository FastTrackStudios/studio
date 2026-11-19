//! The launcher in Architect's navigator: one provider, **Open**, with
//! what there is to open on this machine that no other provider lists.
//!
//! The vault's setlists and libraries are already the setlists
//! extension's providers (`features/setlists/src/nav.rs`), and stay so.
//! This one adds what was opened lately and the rigs, and lists the
//! setlists once more as rows that show the launcher's page for them
//! (the song list, and Open set).

use architect_navigator_api::{Item, NavigatorProvider, Node, Open, ProviderFuture, Snapshot};
use serde_json::json;

use crate::find::Rigs;
use crate::model::{Recent, clock};
use crate::{Found, Loaded};

/// The provider's action prefix (`extension.toml`'s `[[navigator_providers]] actions`).
pub const PREFIX: &str = "STUDIO_LAUNCHER_NAV";

const RECENT: &str = "recent";
const SETLISTS: &str = "setlists";
const RIGS: &str = "rigs";

fn setlist_item(found: &Found) -> Item {
    let list = &found.setlist;
    let count = list.entries().count();
    let (length, _) = list.length();
    let mut about = vec![format!("{count} songs")];
    if length > 0.0 {
        about.push(clock(length));
    }
    about.push(format!("{} with tracks", found.playable()));
    let mut item = Item::new(list.path.clone(), SETLISTS, list.title.clone())
        .icon("list-music")
        .preview(about.join(", "))
        .opens(Open::Action {
            id: "STUDIO_LAUNCHER_CHOOSE".to_owned(),
            args: json!({ "setlist": list.path }),
        });
    if let Some(date) = &list.date {
        item = item.property("date", date.clone());
    }
    item
}

/// The provider's tree, from what was read.
#[must_use]
pub fn snapshot(loaded: &Loaded, recent: &[Recent], rigs: &[Rigs]) -> Snapshot {
    let mut snap = Snapshot::new("Open")
        .ordered()
        .node(Node::new(RECENT, "Recent").icon("history"))
        .node(Node::new(SETLISTS, "Setlists").icon("list-music"));
    let mut rig_node = Node::new(RIGS, "Rigs").icon("guitar");
    for rig in rigs {
        rig_node = rig_node.child(Node::new(
            format!("{RIGS}/{}", rig.instrument),
            rig.instrument.clone(),
        ));
    }
    snap = snap.node(rig_node);
    for (i, r) in recent.iter().enumerate() {
        snap = snap.item(
            Item::new(format!("{RECENT}|{i}"), RECENT, r.name.clone())
                .icon("music")
                .preview(r.when.clone())
                .opens(Open::Action {
                    id: "DAW_OPEN".to_owned(),
                    args: json!({ "path": r.path }),
                }),
        );
    }
    for found in &loaded.setlists {
        snap = snap.item(setlist_item(found));
    }
    for rig in rigs {
        let container = format!("{RIGS}/{}", rig.instrument);
        for profile in &rig.profiles {
            snap = snap.item(
                Item::new(
                    format!("{container}|{profile}"),
                    container.clone(),
                    profile.clone(),
                )
                .icon("sliders-horizontal")
                .preview("Listed only: loading a rig into the engine is not built yet"),
            );
        }
    }
    snap
}

/// What there is to open, as the navigator shows it.
#[derive(Debug, Clone, Copy, Default)]
pub struct LauncherNav;

impl NavigatorProvider for LauncherNav {
    fn snapshot(&self) -> ProviderFuture<Snapshot> {
        Box::pin(async move {
            let loaded = crate::load().await?;
            // No engine: nothing recent to say, the rest still lists.
            let recent = crate::recent().await.unwrap_or_default();
            Ok(snapshot(&loaded, &recent, &crate::find::rigs()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_has_recent_setlists_and_rigs() {
        let recent = [Recent {
            name: "Drive To LA".to_owned(),
            when: "2 hours ago".to_owned(),
            path: "/p/Drive To LA.session".to_owned(),
        }];
        let rigs = [Rigs {
            instrument: "guitar".to_owned(),
            profiles: vec!["worship".to_owned()],
        }];
        let snap = snapshot(&Loaded::default(), &recent, &rigs);
        let titles: Vec<&str> = snap.nodes().iter().map(|(_, n)| n.title.as_str()).collect();
        assert_eq!(titles, ["Recent", "Setlists", "Rigs", "guitar"]);
        assert_eq!(snap.items.len(), 2);
        assert_eq!(snap.items[0].container, "recent");
        assert_eq!(snap.items[1].container, "rigs/guitar");
    }
}
