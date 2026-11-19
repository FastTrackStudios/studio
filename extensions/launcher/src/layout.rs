//! Where the panes go: the two layouts the launcher knows by name, as
//! rewrites of the workspace's own export.
//!
//! Pure: a `WORKSPACE_EXPORT` in, the layout `WORKSPACE_APPLY` takes out.
//! The steps that open the panes and ask the workspace are
//! [`crate::arrange`]. **Stage** is the layout `stack/run-studio.sh` built
//! by hand for a show over three displays; **Rehearse** is one window.
//!
//! An export's node is a group of tabs (`{"group": {"tabs": [{"view"}]}}`)
//! or a split (`{"split": {"axis", "children": [{"share", "node"}]}}`).

use serde_json::{Value, json};

pub const ARRANGEMENT: &str = "studio.arrangement/arrangement";
pub const MIXER: &str = "studio.mixer/mixer";
pub const PROGRESS: &str = "studio.navigator/progress";
pub const NAVIGATOR: &str = "studio.navigator/navigator";
pub const LYRICS: &str = "studio.lyrics/lyrics";
pub const CHART: &str = "keyflow/chart";
pub const SHOW: &str = "ignition/show";
pub const CUES: &str = "ignition/cues";

/// The layouts the launcher makes itself, by name.
pub const BUILT_IN: [&str; 2] = ["Stage", "Rehearse"];

/// The mixer's width beside the arrangement, points: its strips (the top
/// groups of a song).
const MIXER_WIDTH: f64 = 1070.0;
/// The progress bar's height with its foot, points.
const PROGRESS_HEIGHT: f64 = 50.0;
/// A window this much wider than tall puts the mixer beside the
/// arrangement (an ultrawide); narrower, under it.
const WIDE: f64 = 2.4;

/// The views a node holds, in order.
#[must_use]
pub fn views(node: &Value) -> Vec<String> {
    if let Some(tabs) = node.pointer("/group/tabs").and_then(Value::as_array) {
        return tabs
            .iter()
            .filter_map(|t| t.get("view").and_then(Value::as_str).map(str::to_owned))
            .collect();
    }
    node.pointer("/split/children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("node"))
        .flat_map(views)
        .collect()
}

/// The group under `node` that shows `view`.
#[must_use]
pub fn find<'a>(node: &'a Value, view: &str) -> Option<&'a Value> {
    if node.get("group").is_some() {
        return views(node).iter().any(|v| v == view).then_some(node);
    }
    node.pointer("/split/children")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|c| c.get("node"))
        .find_map(|n| find(n, view))
}

/// `first` and `second` along `axis` (`row` or `column`), `first` taking
/// `share` of it.
fn split(axis: &str, first: &Value, second: &Value, share: f64) -> Value {
    let share = share.clamp(0.02, 0.98);
    json!({ "split": { "axis": axis, "children": [
        { "share": share, "node": first },
        { "share": 1.0 - share, "node": second },
    ] } })
}

/// A window's shape, points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub w: f64,
    pub h: f64,
}

impl Area {
    /// The first window's area in a `WORKSPACE_LAYOUT` answer.
    #[must_use]
    pub fn of(layout: &Value) -> Option<Self> {
        let area = layout.pointer("/windows/0/area")?;
        Some(Self {
            w: area.get("w")?.as_f64()?,
            h: area.get("h")?.as_f64().filter(|h| *h > 0.0)?,
        })
    }

    fn wide(self) -> bool {
        self.w / self.h >= WIDE
    }
}

impl Default for Area {
    /// The display the first Stage was laid out on.
    fn default() -> Self {
        Self {
            w: 5120.0,
            h: 1400.0,
        }
    }
}

/// The main window's centre for the stage: the arrangement with the mixer
/// beside it (a wide window) or under it, the set's navigator down the
/// left when it is open, the song's progress bar across the top.
fn stage_main(center: &Value, area: Area) -> Option<Value> {
    let (arrangement, mixer) = (find(center, ARRANGEMENT)?, find(center, MIXER)?);
    let mut work = if area.wide() {
        split("row", arrangement, mixer, 1.0 - MIXER_WIDTH / area.w)
    } else {
        split("column", arrangement, mixer, 0.62)
    };
    if let Some(navigator) = find(center, NAVIGATOR) {
        work = split("row", navigator, &work, 0.07);
    }
    if let Some(progress) = find(center, PROGRESS) {
        work = split("column", progress, &work, PROGRESS_HEIGHT / area.h);
    }
    Some(work)
}

/// The stage: `export` with each window's centre laid out for a show.
///
/// The main window as [`stage_main`]; the lighting's window the room
/// with its cues to the right; the chart's window the chart over the
/// lyrics. A window that holds none of those pairs is left as it is.
#[must_use]
pub fn stage(mut export: Value, area: Area) -> Value {
    let Some(windows) = export.get_mut("windows").and_then(Value::as_array_mut) else {
        return export;
    };
    for window in windows {
        let Some(center) = window.get("center").filter(|c| !c.is_null()).cloned() else {
            continue;
        };
        let pair = |a: &str, b: &str| Some((find(&center, a)?, find(&center, b)?));
        let laid = stage_main(&center, area)
            .or_else(|| pair(SHOW, CUES).map(|(show, cues)| split("row", show, cues, 0.8)))
            .or_else(|| {
                pair(CHART, LYRICS).map(|(chart, lyrics)| split("column", chart, lyrics, 0.67))
            });
        if let (Some(laid), Some(fields)) = (laid, window.as_object_mut()) {
            fields.insert("center".to_owned(), laid);
        }
    }
    export
}

/// Rehearsing: one window, the arrangement over the mixer on the left,
/// the chart over the lyrics on the right. Whichever of them is not open
/// is left out.
#[must_use]
pub fn rehearse(mut export: Value) -> Value {
    let Some(window) = export.pointer_mut("/windows/0") else {
        return export;
    };
    let Some(center) = window.get("center").filter(|c| !c.is_null()).cloned() else {
        return export;
    };
    let column =
        |top: &str, bottom: &str, share: f64| match (find(&center, top), find(&center, bottom)) {
            (Some(top), Some(bottom)) => Some(split("column", top, bottom, share)),
            (Some(only), None) | (None, Some(only)) => Some(only.clone()),
            (None, None) => None,
        };
    let laid = match (
        column(ARRANGEMENT, MIXER, 0.62),
        column(CHART, LYRICS, 0.67),
    ) {
        (Some(work), Some(page)) => Some(split("row", &work, &page, 0.6)),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    };
    if let (Some(laid), Some(fields)) = (laid, window.as_object_mut()) {
        fields.insert("center".to_owned(), laid);
    }
    export
}

/// A layout a person saved: the workspace's export, as it was.
///
/// Its panes keep the ids they had (the chart's names the session it was
/// opened on); the chart is pointed at the engine's song after it is
/// applied ([`crate::follow`]), so nothing in it is rewritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub layout: Value,
}

impl Saved {
    /// As the extension's data keeps it.
    #[must_use]
    pub fn json(&self) -> Value {
        json!({ "layout": self.layout })
    }

    #[must_use]
    pub fn of(value: &Value) -> Option<Self> {
        Some(Self {
            layout: value.get("layout").filter(|l| l.is_object())?.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(views: &[&str]) -> Value {
        let tabs: Vec<Value> = views
            .iter()
            .map(|v| json!({ "view": v, "pane": v }))
            .collect();
        json!({ "group": { "tabs": tabs } })
    }

    fn row(nodes: Vec<Value>) -> Value {
        let children: Vec<Value> = nodes
            .into_iter()
            .map(|n| json!({ "share": 0.5, "node": n }))
            .collect();
        json!({ "split": { "axis": "row", "children": children } })
    }

    fn axis(node: &Value) -> &str {
        node.pointer("/split/axis")
            .and_then(Value::as_str)
            .unwrap_or("")
    }

    fn child(node: &Value, i: usize) -> &Value {
        &node["split"]["children"][i]["node"]
    }

    fn share(node: &Value, i: usize) -> f64 {
        node["split"]["children"][i]["share"].as_f64().unwrap()
    }

    fn export() -> Value {
        json!({ "windows": [
            { "id": "main", "center": row(vec![group(&[ARRANGEMENT]), group(&[MIXER]), group(&[PROGRESS])]) },
            { "id": "w2", "center": row(vec![group(&[CHART]), group(&[LYRICS])]) },
            { "id": "w3", "center": row(vec![group(&[SHOW]), group(&[CUES])]) },
            { "id": "w4", "center": group(&["files"]) },
        ] })
    }

    #[test]
    fn views_and_groups_are_found_through_splits() {
        let center = row(vec![group(&["a", "b"]), row(vec![group(&["c"])])]);
        assert_eq!(views(&center), ["a", "b", "c"]);
        assert_eq!(find(&center, "c"), Some(&group(&["c"])));
        assert_eq!(find(&center, "d"), None);
    }

    #[test]
    fn the_stage_on_an_ultrawide_puts_the_mixer_beside_the_arrangement() {
        let laid = stage(
            export(),
            Area {
                w: 5120.0,
                h: 1400.0,
            },
        );
        let main = &laid["windows"][0]["center"];
        // The progress bar across the top, as tall as it is.
        assert_eq!(axis(main), "column");
        assert_eq!(views(child(main, 0)), [PROGRESS]);
        assert!((share(main, 0) - 50.0 / 1400.0).abs() < 1e-9);
        let work = child(main, 1);
        assert_eq!(axis(work), "row");
        assert_eq!(views(work), [ARRANGEMENT, MIXER]);
        assert!((share(work, 1) - 1070.0 / 5120.0).abs() < 1e-9);
        // The chart over the lyrics, the room beside its cues.
        let chart = &laid["windows"][1]["center"];
        assert_eq!(
            (axis(chart), views(chart)),
            ("column", vec![CHART.to_owned(), LYRICS.to_owned()])
        );
        let room = &laid["windows"][2]["center"];
        assert_eq!((axis(room), share(room, 0)), ("row", 0.8));
        // A window of something else is left alone.
        assert_eq!(laid["windows"][3], export()["windows"][3]);
    }

    #[test]
    fn the_stage_on_a_16_by_9_puts_the_mixer_under_it() {
        let laid = stage(
            export(),
            Area {
                w: 2560.0,
                h: 1440.0,
            },
        );
        let work = child(&laid["windows"][0]["center"], 1);
        assert_eq!(axis(work), "column");
        assert!((share(work, 0) - 0.62).abs() < 1e-9);
    }

    #[test]
    fn rehearsing_is_one_window_of_work_and_page() {
        let one = json!({ "windows": [ { "id": "main", "center": row(vec![
            group(&[CHART]), group(&[ARRANGEMENT]), group(&[LYRICS]), group(&[MIXER]),
        ]) } ] });
        let laid = rehearse(one);
        let center = &laid["windows"][0]["center"];
        assert_eq!(axis(center), "row");
        assert_eq!(views(child(center, 0)), [ARRANGEMENT, MIXER]);
        assert_eq!(views(child(center, 1)), [CHART, LYRICS]);
        // With no chart open, the work alone.
        let bare = json!({ "windows": [ { "center": row(vec![group(&[MIXER]), group(&[ARRANGEMENT])]) } ] });
        assert_eq!(
            views(&rehearse(bare)["windows"][0]["center"]),
            [ARRANGEMENT, MIXER]
        );
    }

    #[test]
    fn a_saved_layout_is_kept_as_it_was() {
        let saved = Saved {
            layout: json!({ "windows": [ { "center": group(&[CHART]) } ] }),
        };
        assert_eq!(Saved::of(&saved.json()), Some(saved));
        assert_eq!(Saved::of(&json!({ "layout": "none" })), None);
    }

    #[test]
    fn an_area_is_read_from_the_layout() {
        let layout = json!({ "windows": [ { "area": { "w": 1920.0, "h": 1080.0 } } ] });
        assert_eq!(
            Area::of(&layout),
            Some(Area {
                w: 1920.0,
                h: 1080.0
            })
        );
        assert_eq!(Area::of(&json!({})), None);
    }
}
