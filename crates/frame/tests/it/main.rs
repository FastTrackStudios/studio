//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod authoring;
#[cfg(all(feature = "figma", feature = "io"))]
mod bundle;
#[cfg(all(feature = "theme"))]
mod components;
#[cfg(all(feature = "design", feature = "runtime"))]
mod controls;
#[cfg(all(feature = "design"))]
mod design_lint_motion;
mod effects;
#[cfg(all(feature = "runtime"))]
mod end_to_end;
#[cfg(all(
    feature = "design",
    feature = "library",
    feature = "render",
    feature = "runtime"
))]
mod eq_graph;
mod expr;
#[cfg(all(feature = "figma", feature = "render"))]
mod golden_primitives;
#[cfg(all(feature = "cpu", feature = "embed", feature = "runtime"))]
mod harness;
mod interact;
#[cfg(all(feature = "design", feature = "render", feature = "runtime"))]
mod knobs;
#[cfg(all(feature = "figma"))]
mod law;
#[cfg(all(feature = "theme"))]
mod layers;
#[cfg(all(feature = "render", feature = "runtime"))]
mod lettering;
#[cfg(all(feature = "figma", feature = "io"))]
mod loro_scene;
#[cfg(all(feature = "runtime"))]
mod materials;
#[cfg(all(feature = "design", feature = "runtime"))]
mod meters;
#[cfg(all(feature = "runtime"))]
mod meters_and_readouts;
mod model;
mod motion;
#[cfg(all(feature = "anyrender", feature = "render", feature = "runtime"))]
mod needle_subtree;
#[cfg(all(feature = "io", feature = "runtime"))]
mod persistence;
#[cfg(all(feature = "render", feature = "runtime"))]
mod plots_and_handles;
#[cfg(all(feature = "runtime", feature = "surface-runtime-rpc"))]
mod remote;
#[cfg(all(feature = "design", feature = "runtime"))]
mod rules;
#[cfg(all(feature = "theme"))]
mod shapes;
#[cfg(all(feature = "theme"))]
mod slots;
#[cfg(all(feature = "runtime"))]
mod stepped;
#[cfg(all(feature = "theme"))]
mod strip;
#[cfg(all(feature = "runtime"))]
mod surface_kit_motion;
#[cfg(all(feature = "runtime"))]
mod surface_runtime_motion;
mod theming;
#[cfg(all(feature = "design", feature = "render", feature = "runtime"))]
mod viz;
#[cfg(all(feature = "design", feature = "render", feature = "runtime"))]
mod viz_lit;
