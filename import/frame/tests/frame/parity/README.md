# Parity Scenario Pack

This directory is the canonical input for render-parity testing.

## Scenario Layout

Each scenario lives in:

`tests/frame/parity/scenarios/<scenario-id>/`

Required files:

- `export/latest-bridge-export.json`: frame export JSON for this case
- `expected/*.png`: direct Figma PNG exports (reference truth)
- `actual/*.png`: renderer/plugin PNG outputs to compare

Example:

- `tests/frame/parity/scenarios/paint.alpha-steps/export/latest-bridge-export.json`
- `tests/frame/parity/scenarios/paint.alpha-steps/expected/frame.png`
- `tests/frame/parity/scenarios/paint.alpha-steps/actual/frame.png`

`expected` and `actual` are compared by relative PNG path.

## Naming Convention

- Scenario id: `<category>.<case>`
- Figma frame name: `parity/<category>/<case>`
- Export JSON filename: always `latest-bridge-export.json`

## Figma Export Rules (for deterministic pixel diffs)

- PNG scale: `1x`
- Keep frame bounds fixed between exports
- Freeze fonts/assets before export
- Do not rename PNG files between `expected` and `actual`
- Final threshold target: `0.0` diff percent

## Core Scenario Matrix

Create at least one frame for each id:

- `primitives.basic-shapes`
- `primitives.fractional-geometry`
- `primitives.corner-radius-extremes`
- `paint.alpha-steps`
- `paint.gradient-matrix`
- `paint.blend-mode-stack`
- `paint.nested-opacity-stack`
- `effects.shadow-matrix`
- `effects.layer-background-blur`
- `effects.effect-order-stack`
- `strokes.caps-joins-dashes`
- `strokes.open-closed-paths`
- `strokes.boolean-ops`
- `masks.clip-content-nesting`
- `masks.vector-text-image-mask`
- `masks.mask-effects-transforms`
- `images.fill-mode-matrix`
- `images.scale-extremes`
- `images.alpha-over-gradient`
- `images.crop-rotate-clip`
- `text.single-multiline-wrap`
- `text.alignment-truncation`
- `text.mixed-style-runs`
- `layout.auto-layout-hug-fill`
- `layout.constraints-min-max`
- `layout.absolute-in-auto-layout`
- `transform.translate-scale-rotate`
- `transform.nested-transform-stack`
- `stress.deep-hierarchy`
- `stress.many-siblings`
- `stress.large-canvas`
- `edge.zero-size-nodes`
- `edge.subpixel-tiny-geometry`
- `edge.negative-offcanvas`
- `edge.transparent-overlap-blend`
- `regression.real-screen-1`
- `regression.real-screen-2`
- `regression.kitchen-sink`

## Running

All scenarios:

```bash
tools/run_parity_scenarios.sh --threshold 0.0
```

Single scenario:

```bash
tools/run_parity_scenarios.sh --scenario paint.alpha-steps --threshold 0.0
```

Reports are written under:

- `tests/frame/parity/reports/<scenario-id>/`
- `tests/frame/parity/reports/summary.json`
- `tests/frame/parity/reports/summary.md`

## Manifest Template

Use and customize:

- `tests/frame/parity/scenarios/manifest.template.json`

You can copy it to `manifest.json` for team tracking.
