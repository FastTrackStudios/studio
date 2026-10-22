//! The shared studio shell (feature `ui`): one frame for Session and Signal,
//! desktop and iPad — an app bar, left and right sidebars, the main view,
//! and top and bottom drawers — plus the panel registry that lets panels
//! from any domain mount in any region they're allowed in.
//!
//! Design: `docs/design/unified-shell/spec.md` (STA-152). Plan: the
//! STA-153 audit §4 — this is its stage 1 (frame + state) and the registry
//! half of stage 2. Not here yet, by design: size classes and phone
//! overlays (spec §3), drag-to-dock (spec §10 step 6, on `dock`), and the
//! shared `TransportSource`/`SetlistSource` services (audit §4.1 stage 2).
//!
//! One shell, in layers:
//! - [`crate::tokens`]: the look (colours, sizes, font), shared with the
//!   iPad rig frame.
//! - this module: the regions, their saved layout and the panel registry
//!   (what goes where, open or collapsed).
//! - `signal-rig-shell`: the iPad rig's frame pieces (top bar with sidebar
//!   tabs, foot bar, dock), drawn with the same tokens. They are region
//!   *contents*; porting the iPad `MobileApp` onto [`ShellFrame`] (audit
//!   stage 3.2) mounts them in its app bar, left sidebar and bottom drawer.
//! - `signal-rig-perform-ui`: the rigs' performance screens (Setlist,
//!   Preset, Profile) — panels, to be registered here once they mount
//!   outside the rig frame.
//!
//! ```ignore
//! #[component]
//! fn Workspace() -> Element {
//!     use_shell_state("myapp.workspace");
//!     use_panel_registry(|| {
//!         PanelRegistry::new()
//!             .with(Panel::new("x.list", "List", Region::Left, ListPanel))
//!             .with(Panel::new("x.view", "View", Region::Main, ViewPanel))
//!     });
//!     rsx! { ShellFrame {} }
//! }
//! ```

mod frame;
mod layout;
mod registry;

pub use frame::{
    RegionHost, ShellFrame, ShellState, load_layout, use_panel_registry, use_shell, use_shell_state,
};
pub use layout::{Region, RegionLayout, ShellLayout, pref_key};
pub use registry::{Panel, PanelConstraints, PanelRegistry, Render};
