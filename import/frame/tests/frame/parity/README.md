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

## Core 10 Matrix

Start with these 10 scenarios only:

- `primitives.fractional-geometry`
- `paint.gradient-matrix`
- `paint.blend-mode-stack`
- `effects.layer-background-blur`
- `strokes.caps-joins-dashes`
- `masks.vector-text-image-mask`
- `images.fill-mode-matrix`
- `text.mixed-style-runs`
- `layout.auto-layout-hug-fill`
- `regression.kitchen-sink`

Add more scenarios later only after the Core 10 set is stable.

## Running

Render renderer `actual/frame.png` outputs first:

```bash
tools/render_parity_actuals.sh --all
```

Core 10 only (default):

```bash
tools/run_parity_scenarios.sh --threshold 0.0
```

All scenarios:

```bash
tools/run_parity_scenarios.sh --all --threshold 0.0
```

Single scenario:

```bash
tools/run_parity_scenarios.sh --scenario paint.gradient-matrix --threshold 0.0
```

One command for render + diff/report:

```bash
tools/run_parity_render_and_diff.sh --all --threshold 0.0
tools/run_parity_render_and_diff.sh --scenario paint.gradient-matrix --threshold 0.0
cargo xtask parity_test --all --threshold 0.0
cargo xtask parity_test --scenario paint.gradient-matrix --threshold 0.0
```

Reports are written under:

- `tests/frame/parity/reports/<scenario-id>/`
- `tests/frame/parity/reports/summary.json`
- `tests/frame/parity/reports/summary.md`

## Manifest Template

Use and customize:

- `tests/frame/parity/scenarios/manifest.template.json`

You can copy it to `manifest.json` for team tracking.

## Detailed Build Spec

Use the exact per-scenario layer contents from:

- `docs/parity-scenario-contents-spec.md`
