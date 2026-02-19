# Parity Scenario Contents Spec

This document defines exactly what to place in each Figma frame for parity testing.

## Global Rules (all scenarios)

1. Frame name must be exactly `parity/<category>/<case>`.
2. Frame size defaults to `1920x1080` unless scenario says otherwise.
3. Use PNG export at `1x` only.
4. Keep a 3-row structure in each frame:
- Row A: baseline behavior.
- Row B: edge/extreme values.
- Row C: mixed/interaction stress.
5. Add small labels (`12px`) above each test group with its mini-case name.
6. Use fixed neutral background `#F5F5F5` unless scenario needs transparency checks.

## Primitive Geometry

### `primitives.basic-shapes`
- Row A: rectangle, ellipse, line, star, polygon at integer coordinates.
- Row B: same shapes with different fills/strokes.
- Row C: overlapping versions to verify paint order.

### `primitives.fractional-geometry`
- Row A: shapes at `.5` positions and fractional sizes.
- Row B: 1px strokes on subpixel positions.
- Row C: mixed integer + fractional neighbors for aliasing contrast.

### `primitives.corner-radius-extremes`
- Row A: rectangles with uniform radius (0, 4, 16, 64).
- Row B: per-corner radii with asymmetric values.
- Row C: radii near/over half-dimension clamp limits.

## Paint and Compositing

### `paint.alpha-steps`
- Row A: solid swatches at alpha `1.0`, `0.75`, `0.5`, `0.25`, `0.01`.
- Row B: same swatches over checkerboard background.
- Row C: nested groups with parent opacity + child opacity combinations.

### `paint.gradient-matrix`
- Row A: linear gradients (2-stop, 4-stop, hard stop).
- Row B: radial and angular gradients with rotated transforms.
- Row C: gradient + stroke + effect combos.

### `paint.blend-mode-stack`
- Row A: normal/multiply/screen/overlay over colored backdrop.
- Row B: darken/lighten/color-burn/soft-light set.
- Row C: 3-layer stack mixing blend modes.

### `paint.nested-opacity-stack`
- Row A: single group opacity variants.
- Row B: parent+child opacity multiplication tests.
- Row C: opacity + blend mode interaction.

## Effects

### `effects.shadow-matrix`
- Row A: drop shadows (small/medium/large blur).
- Row B: inner shadows (small/medium/large blur).
- Row C: multiple shadows per node with different colors/opacities.

### `effects.layer-background-blur`
- Row A: layer blur values `2`, `8`, `24`.
- Row B: background blur cards over patterned background.
- Row C: blur + rounded corners + clipping.

### `effects.effect-order-stack`
- Row A: fill + stroke + shadow.
- Row B: fill + blur + shadow.
- Row C: clip + blur + shadow + opacity to test ordering.

## Strokes and Vector Paths

### `strokes.caps-joins-dashes`
- Row A: caps (butt/round/square) on identical lines.
- Row B: joins (miter/round/bevel) on sharp polyline.
- Row C: dashed lines with varying dash/gap/offset.

### `strokes.open-closed-paths`
- Row A: open paths with stroke only.
- Row B: closed paths with fill+stroke.
- Row C: self-intersecting path edge cases.

### `strokes.boolean-ops`
- Row A: union/subtract/intersect/exclude simple pairs.
- Row B: nested boolean groups.
- Row C: boolean result with gradients/strokes applied.

## Masks and Clipping

### `masks.clip-content-nesting`
- Row A: single clipped frame with overflowing children.
- Row B: two-level nested clips.
- Row C: clip + transform (rotated/scaled child).

### `masks.vector-text-image-mask`
- Row A: vector mask over color blocks.
- Row B: text mask over gradient/image.
- Row C: image mask with transparency regions.

### `masks.mask-effects-transforms`
- Row A: masked node with drop shadow.
- Row B: masked node with blur.
- Row C: masked + rotated + blended stack.

## Images

### `images.fill-mode-matrix`
- Row A: same image in fill/fit/crop/tile modes.
- Row B: image with different frame aspect ratios.
- Row C: image fill + stroke + corner radius.

### `images.scale-extremes`
- Row A: very large source scaled down.
- Row B: very small source scaled up.
- Row C: non-uniform scaling and crop offsets.

### `images.alpha-over-gradient`
- Row A: transparent PNG over linear gradient.
- Row B: transparent PNG over radial gradient.
- Row C: stacked transparent PNGs over blend layers.

### `images.crop-rotate-clip`
- Row A: cropped images with known focal point.
- Row B: rotated image inside clip frame.
- Row C: crop + rotate + rounded clip combined.

## Text

### `text.single-multiline-wrap`
- Row A: single-line labels with varying sizes.
- Row B: multiline paragraphs with fixed-width wrap.
- Row C: mixed long/short strings including punctuation.

### `text.alignment-truncation`
- Row A: left/center/right and top/middle/bottom alignment.
- Row B: truncation/overflow behavior cases.
- Row C: alignment in constrained auto-layout containers.

### `text.mixed-style-runs`
- Row A: one text node with multiple weights/colors.
- Row B: mixed style spans + letter spacing changes.
- Row C: mixed styles across wrapped lines.

## Layout and Constraints

### `layout.auto-layout-hug-fill`
- Row A: row auto-layout with hug contents.
- Row B: column auto-layout with fill container children.
- Row C: nested auto-layout with mixed hug/fill/fixed.

### `layout.constraints-min-max`
- Row A: left/right/top/bottom pin constraints.
- Row B: center/scale constraints.
- Row C: min/max-like behavior via resize stress (document expected behavior in label).

### `layout.absolute-in-auto-layout`
- Row A: absolute-position child inside auto-layout parent.
- Row B: multiple absolute children with z-order.
- Row C: absolute + transformed child in auto-layout.

## Transforms

### `transform.translate-scale-rotate`
- Row A: translation-only set.
- Row B: scale-only set (uniform/non-uniform).
- Row C: rotation around different origins.

### `transform.nested-transform-stack`
- Row A: parent rotate + child translate.
- Row B: parent scale + child rotate.
- Row C: 3-level nested transforms.

## Stress

### `stress.deep-hierarchy`
- Row A: 10+ nested groups with visible content at each level.
- Row B: nested clips and transforms.
- Row C: nested effects and opacity.

### `stress.many-siblings`
- Row A: 200+ sibling nodes in one parent.
- Row B: mixed node types with repeated styles.
- Row C: overlapping sibling layers for paint-order stress.

### `stress.large-canvas`
- Use `4096x4096` frame for this scenario.
- Row A: sparse content across corners.
- Row B: dense center cluster.
- Row C: very large gradients/images/vectors.

## Edge Cases

### `edge.zero-size-nodes`
- Row A: width=0 or height=0 shapes.
- Row B: zero-size inside auto-layout.
- Row C: zero-size with stroke/effect applied.

### `edge.subpixel-tiny-geometry`
- Row A: tiny shapes (`<1px`, `1px`, `1.5px`).
- Row B: tiny strokes on fractional coords.
- Row C: dense tiny pattern block.

### `edge.negative-offcanvas`
- Row A: nodes partially outside frame bounds.
- Row B: nodes fully outside but in hierarchy.
- Row C: off-canvas with effects/masks.

### `edge.transparent-overlap-blend`
- Row A: many translucent layers overlap.
- Row B: translucency + blend modes.
- Row C: translucency + gradient + effect stack.

## Regression Screens

### `regression.real-screen-1`
- One actual product screen with known renderer complexity.
- Include text, images, gradients, effects, and auto-layout.
- Freeze all assets used by the screen.

### `regression.real-screen-2`
- Second real screen with different composition style.
- Include at least one mask and one blur.
- Include at least one transformed group.

### `regression.kitchen-sink`
- Single frame containing one block from each major category.
- Must include: gradient, image fill, blend mode, mask, blur, complex text, auto-layout, nested transform.
- Use this as the final broad regression gate.

## Export Checklist (per scenario)

1. Select only the target frame.
2. Export JSON as `export/latest-bridge-export.json`.
3. Export direct Figma PNG(s) to `expected/`.
4. Render plugin/engine PNG(s) to `actual/` with identical filenames.
5. Run: `tools/run_parity_scenarios.sh --scenario <id> --threshold 0.0`.
