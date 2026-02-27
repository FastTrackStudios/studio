# Ralph Progress Log

This file tracks progress across iterations. Agents update this file
after each iteration and it's included in prompts for context.

## Codebase Patterns (Study These First)

### SVG Color + Opacity Pipeline
- `svg_color_paint_values(r: u8, g: u8, b: u8, a: f64) -> (String, Option<String>)` — encodes RGB as hex, alpha via `fmt_f64(a)` (trims trailing zeros)
- `to_u8(v: f64) -> u8` — only used for RGB channels (hex rendering); never for alpha in SVG output
- `fmt_f64(v: f64) -> String` — 6 decimal places, trimmed (e.g., 0.7 not 0.700000)
- Rgba struct stores all channels as f64; alpha flows through without quantization in SVG path

### Parity Test Workflow
- `tools/render_parity_actuals.sh --all` — regenerates all actual.svg files
- `tools/run_svg_parity_scenarios.sh --all` — runs comparison and updates reports
- `actual.svg` files in scenario dirs are generated artifacts; can be stale after code changes
- Failures in: effects.effect-ordering, effects.layer-background-blur, primitives.fractional-geometry, regression.tough-1176-blue-face are pre-existing (not introduced by bd-der.1)

---

## 2026-02-27 - bd-der.1
- **What was implemented**: Eliminated 8-bit opacity quantization in SVG export
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — changed `svg_color_paint_values` signature from `a: u8` to `a: f64`, updated all call sites to pass `color.a` directly instead of `(color.a * 255.0).round() as u8`
  - `crates/frame-ui/src/svg/effects.rs` — same fix for stroke color alpha in push_rect_with_effects
  - `tests/frame/parity/scenarios/regression.tough-1176-blue-face/actual.svg` — regenerated with clean opacity values
  - `tests/frame/parity/scenarios/text.alignment-truncation/actual.svg` — regenerated, now passes strict parity
  - Report files updated
- **Learnings**:
  - The fix was already present in the working tree (previous iteration had implemented it); this iteration verified correctness and regenerated stale artifacts
  - The old code did `(color.a * 255.0).round().clamp(0.0, 255.0) as u8` as the 4th arg to svg_color_paint_values, which stored alpha as u8 and then formatted as `f64::from(a) / 255.0` with 6 decimal places — creating values like 0.698039 instead of 0.7
  - Figma JSON opacity values come in as f32-precision (e.g., 0.699999988079071 for 0.7); `fmt_f64` correctly rounds these to "0.7" via `format!("{:.6}", v)` + trim trailing zeros
  - actual.svg parity files are GENERATED artifacts — always regenerate after code changes before running comparison
  - text.alignment-truncation was also fixed as a side effect (now strict=true)

---

## 2026-02-27 - bd-der.3
- **What was implemented**: Placed opacity on `<g filter="...">` wrapper instead of baking it into fill/effect colors
- **Files changed**:
  - `crates/frame-ui/src/anyrender_renderer/collect.rs` — added `has_filter_effects` early check; `needs_layer` now also true when `has_filter_effects && effective_opacity < 1.0`, so opacity is NOT baked into fills/effects for those nodes
  - `crates/frame-ui/src/svg/render.rs` — modified `LayerEnd` to merge outer opacity group into inner filter group child; added `try_merge_layer_into_filter_child()` and `push_node_dyn()` helpers
- **Learnings**:
  - Figma's SVG convention: when a node has both opacity AND filter effects, opacity goes on the SAME `<g filter="...">` element, NOT baked into fill/stroke/effect colors
  - The `svg` crate's `Node` trait provides `get_attributes_mut()` and `get_children_mut()` for DOM inspection — `Box<dyn Node>` is usable via trait object dispatch
  - `Box<dyn Node>: Into<Box<dyn Node>>` via std identity impl, so `group.add(child_box)` compiles
  - Pre-existing `needs_layer` in collect.rs was only for non-Normal blend modes; adding filter-effects case required peeking at raw JSON before full effect parsing
  - The merge check: require outer group has opacity, exactly 1 child, child is `<g>` with `filter` but no `opacity`; copy all outer attrs to child using `entry().or_insert()`
  - SVG attribute order differs (actual: `filter opacity`, expected: `opacity filter`) but is semantically equivalent — canonical diff still succeeds
  - 22/26 parity tests still pass; same 4 pre-existing failures remain unchanged

---

## 2026-02-27 - bd-der.4
- **What was implemented**: Applied rotation transform to star and vector shape paths
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — inserted rotation block after px/py candidate correction (before bounds accumulation). When `rotation.abs() > 1e-6 && has_native_geometry && !text_outline`, applies 2×2 rotation to all fill_paths/stroke_paths via `kurbo::Affine` and recomputes `px/py` as `chain_sx/chain_sy`
- **Key formulas**:
  - `cos_r = rotation.cos()`, `sin_r = rotation.sin()` (rotation from extract_local_rect = atan2(c, a))
  - `abs_cx = bx + tx + bw/2`, `abs_cy = by + ty + bh/2` (absoluteBoundingBox center in screen space)
  - `chain_sx = abs_cx - cos_r*(w/2) + sin_r*(h/2)` — positions rotated local center at abs center
  - `chain_sy = abs_cy - sin_r*(w/2) - cos_r*(h/2)`
  - `kurbo::Affine::new([cos_r, sin_r, -sin_r, cos_r, 0.0, 0.0])` — column-major 2×2 rotation only
- **Learnings**:
  - Figma's relativeTransform tx/ty is the AABB top-left, NOT the local origin — the correct translate uses absoluteBoundingBox center
  - Decompose affine = 2×2 rotation on path coords + translate; normalize bakes the translate into compact path format
  - kurbo Affine::new([a,b,c,d,e,f]) is column-major: `|a c e| / |b d f|`; Figma row-major [[cos_r, -sin_r],[sin_r, cos_r]] → `new([cos_r, sin_r, -sin_r, cos_r, 0, 0])`
  - `match primitive` (not `match &primitive`) means fill_paths/stroke_paths are owned Vec<String>; shadow with `let (fill_paths, stroke_paths) = ...;` after candidate correction
  - 22/26 parity tests still pass; same 4 pre-existing failures; star M56.3732 37.7679 and Union paths now match expected.svg exactly

---

