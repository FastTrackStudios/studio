//! The menu tree on the session bus, as `com.canonical.dbusmenu`.
//!
//! It is served at [`PATH`], where the Loft shell finds it by this
//! process's pid (and, where there is a `com.canonical.AppMenu.Registrar`,
//! it is registered with it).
//!
//! A thin skin: the tree is [`super::layout`]'s, and a click runs the item's
//! action through whatever runner it was handed — in the app,
//! [`run_action`], the Action List the keys and a model run.

use std::collections::HashMap;
use std::sync::Arc;

use zbus::fdo;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{Array, Dict, ObjectPath, OwnedValue, Signature, StructureBuilder, Value};

use super::{Node, Prop};

/// Where the menu is served — the path Qt's menubars use, which is what a
/// shell looks for.
pub const PATH: &str = "/MenuBar/1";

/// The layout's revision. The tree is built once, from a registry that
/// is filled once, so it never changes.
const REVISION: u32 = 1;

/// Runs a clicked item's action, by id.
pub type Runner = Arc<dyn Fn(&str) + Send + Sync>;

/// A node as `GetLayout` gives it: `(ia{sv}av)`.
pub type Layout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

/// Serve Session's menu on the session bus, on a thread of its own, for
/// the life of the process. Returns at once; with no session bus to
/// serve on it says so once and gives up.
pub fn start() {
    let spawned = std::thread::Builder::new()
        .name("app-menu".into())
        .spawn(serve_for_good);
    if let Err(error) = spawned {
        tracing::warn!(app_menu.outcome = "no_thread", error = %error, "app menu: not served");
    }
}

/// The thread's body: the tree from the registry and the keymap, served
/// until the process ends.
fn serve_for_good() {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            tracing::warn!(app_menu.outcome = "no_runtime", error = %error, "app menu: not served");
            return;
        }
    };
    runtime.block_on(async {
        let tree = menu_bar();
        let items = tree
            .walk()
            .filter(|n| matches!(n.kind, super::Kind::Item { .. }))
            .count();
        let menus = tree.children.len();
        let connection = match zbus::Connection::session().await {
            Ok(connection) => connection,
            Err(error) => {
                tracing::warn!(app_menu.outcome = "no_session_bus", error = %error, "app menu: not served");
                return;
            }
        };
        if let Err(error) = serve(&connection, tree, Arc::new(run_action)).await {
            tracing::warn!(app_menu.outcome = "not_served", error = %error, "app menu: not served");
            return;
        }
        let registered = register(&connection).await;
        tracing::info!(
            app_menu.outcome = "served",
            app_menu.path = PATH,
            app_menu.menus = menus,
            app_menu.items = items,
            app_menu.registrar = registered,
            "app menu served"
        );
        std::future::pending::<()>().await;
    });
}

/// Session's menu bar: the Action List's actions, with the keymap's keys.
fn menu_bar() -> Node {
    let keyed = super::keyed(crate::keys::Keys::load().bindings());
    super::layout(&super::entries(&crate::actions::list().infos(), &keyed))
}

/// Run a clicked action as a key would: through the Action List, off the
/// bus's thread so a long one (a save) holds nothing up. A window action
/// queues for the window, which the shell's frame loop carries out.
fn run_action(id: &str) {
    if let Err(error) = crate::actions::run(id, b"{}") {
        tracing::warn!(action.id = id, error = %error, "menu action refused");
    }
}

/// Put `tree` at [`PATH`] on `connection`, clicks going to `run`.
///
/// # Errors
///
/// Something is already served there.
pub async fn serve(connection: &zbus::Connection, tree: Node, run: Runner) -> zbus::Result<()> {
    connection
        .object_server()
        .at(PATH, DbusMenu { tree, run })
        .await
        .map(|_| ())
}

/// Tell a registrar, if there is one, which object is the window's menu
/// (window id 0: Wayland has none to give; a shell matches by pid).
/// Whether one took it.
async fn register(connection: &zbus::Connection) -> bool {
    let Ok(path) = ObjectPath::try_from(PATH) else {
        return false;
    };
    connection
        .call_method(
            Some("com.canonical.AppMenu.Registrar"),
            "/com/canonical/AppMenu/Registrar",
            Some("com.canonical.AppMenu.Registrar"),
            "RegisterWindow",
            &(0_u32, path),
        )
        .await
        .is_ok()
}

/// The object a shell talks to.
struct DbusMenu {
    tree: Node,
    run: Runner,
}

impl DbusMenu {
    fn node(&self, id: i32) -> fdo::Result<&Node> {
        self.tree
            .find(id)
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("no menu item {id}")))
    }

    /// Run the item `id` if it is one — `Err` when there is no such node.
    fn click(&self, id: i32) -> fdo::Result<()> {
        self.node(id)?;
        if let Some(action) = self.tree.action(id) {
            let run = Arc::clone(&self.run);
            let action = action.to_owned();
            tokio::task::spawn_blocking(move || run(&action));
        }
        Ok(())
    }
}

#[expect(
    clippy::needless_pass_by_value,
    clippy::unused_self,
    reason = "the D-Bus method's shape: zbus decodes owned arguments and calls through `self`"
)]
#[zbus::interface(name = "com.canonical.dbusmenu")]
impl DbusMenu {
    /// The tree under `parent_id`, `recursion_depth` levels down (-1:
    /// all of it), with `property_names` (none: every one).
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> fdo::Result<(u32, Layout)> {
        let node = self.node(parent_id)?;
        Ok((REVISION, layout(node, recursion_depth, &property_names)))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let nodes: Vec<&Node> = if ids.is_empty() {
            self.tree.walk().collect()
        } else {
            ids.iter().filter_map(|id| self.tree.find(*id)).collect()
        };
        nodes
            .into_iter()
            .map(|node| (node.id, props(node, &property_names)))
            .collect()
    }

    fn get_property(&self, id: i32, name: &str) -> fdo::Result<OwnedValue> {
        let node = self.node(id)?;
        node.props()
            .into_iter()
            .find(|(key, _)| *key == name)
            .and_then(|(_, prop)| owned(prop_value(prop)))
            .ok_or_else(|| fdo::Error::InvalidArgs(format!("menu item {id} has no {name}")))
    }

    /// `clicked` runs the item; anything else (`hovered`, `opened`,
    /// `closed`) needs nothing.
    fn event(&self, id: i32, event_id: &str, data: OwnedValue, timestamp: u32) -> fdo::Result<()> {
        // What was clicked with, and when: nothing a click here needs.
        drop((data, timestamp));
        if event_id == "clicked" {
            self.click(id)
        } else {
            self.node(id).map(|_| ())
        }
    }

    /// Each of `events` as [`Self::event`]; the ids that were not there.
    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        events
            .into_iter()
            .filter_map(|(id, event_id, _, _)| {
                let done = if event_id == "clicked" {
                    self.click(id)
                } else {
                    self.node(id).map(|_| ())
                };
                done.is_err().then_some(id)
            })
            .collect()
    }

    /// Nothing to bring up to date before a menu opens.
    fn about_to_show(&self, id: i32) -> fdo::Result<bool> {
        self.node(id).map(|_| false)
    }

    /// No updates needed; the ids that were not there.
    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        let missing = ids
            .into_iter()
            .filter(|id| self.tree.find(*id).is_none())
            .collect();
        (Vec::new(), missing)
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;

    #[zbus(property)]
    const fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "normal".to_owned()
    }

    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".to_owned()
    }

    #[zbus(property)]
    const fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }
}

/// `node` as `GetLayout` gives it, `depth` levels of children down.
fn layout(node: &Node, depth: i32, names: &[String]) -> Layout {
    let children = child_nodes(node, depth)
        .filter_map(|(child, deeper)| owned(Value::Structure(structure(child, deeper, names)?)))
        .collect();
    (node.id, props(node, names), children)
}

/// The children a layout `depth` levels deep includes, each with the depth
/// left under it.
fn child_nodes(node: &Node, depth: i32) -> impl Iterator<Item = (&Node, i32)> {
    let deeper = if depth < 0 {
        -1
    } else {
        depth.saturating_sub(1)
    };
    node.children
        .iter()
        .filter(move |_| depth != 0)
        .map(move |child| (child, deeper))
}

/// A child of a layout: the same `(ia{sv}av)`, as a structure whose
/// props and children are variants.
fn structure(
    node: &Node,
    depth: i32,
    names: &[String],
) -> Option<zbus::zvariant::Structure<'static>> {
    let mut dict = Dict::new(&Signature::Str, &Signature::Variant);
    for (key, prop) in wanted(node, names) {
        dict.append(Value::from(key), Value::Value(Box::new(prop_value(prop))))
            .ok()?;
    }
    let mut children = Array::new(&Signature::Variant);
    for (child, deeper) in child_nodes(node, depth) {
        let child = structure(child, deeper, names)?;
        children
            .append(Value::Value(Box::new(Value::Structure(child))))
            .ok()?;
    }
    StructureBuilder::new()
        .add_field(node.id)
        .append_field(Value::Dict(dict))
        .append_field(Value::Array(children))
        .build()
        .ok()
}

/// A node's properties as a dbusmenu dict, only `names` (none: all).
fn props(node: &Node, names: &[String]) -> HashMap<String, OwnedValue> {
    wanted(node, names)
        .into_iter()
        .filter_map(|(key, prop)| Some((key.to_owned(), owned(prop_value(prop))?)))
        .collect()
}

/// A node's properties among `names` (none: all).
fn wanted(node: &Node, names: &[String]) -> Vec<(&'static str, Prop)> {
    node.props()
        .into_iter()
        .filter(|(key, _)| names.is_empty() || names.iter().any(|n| n == key))
        .collect()
}

fn prop_value(prop: Prop) -> Value<'static> {
    match prop {
        Prop::Str(s) => Value::from(s),
        Prop::Keys(keys) => Value::from(keys),
    }
}

/// A value without borrows; `None` only for a file descriptor, which no
/// menu property is.
fn owned(value: Value<'static>) -> Option<OwnedValue> {
    OwnedValue::try_from(value).ok()
}
