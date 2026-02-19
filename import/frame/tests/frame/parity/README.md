# Parity Scenario Tracker

This file tracks render parity status across all 24 scenarios.

## Report Locations

Per-scenario report is generated inside each scenario folder:

- `tests/frame/parity/scenarios/<scenario-id>/report.json`

Aggregate run summary is written to:

- `tests/frame/parity/reports/summary.json`
- `tests/frame/parity/reports/summary.md`

## Scenario Status

Fill `Status` after running reports (`pass`, `fail`, `blocked`, etc.).

| Scenario ID | Status | Notes |
| --- | --- | --- |
| `effects.background-blur-stress` |  |  |
| `effects.drop-shadow-matrix` |  |  |
| `effects.effect-ordering` |  |  |
| `effects.inner-shadow-matrix` |  |  |
| `effects.layer-background-blur` |  |  |
| `effects.shadow-matrix` |  |  |
| `images.fill-mode-matrix` |  |  |
| `images.image-sampling` |  |  |
| `layout.auto-layout-hug-fill` |  |  |
| `masks.clip-content-nesting` |  |  |
| `masks.mask-clip-interactions` |  |  |
| `masks.vector-text-image-mask` |  |  |
| `paint.blend-mode-combos` |  |  |
| `paint.blend-mode-stack` |  |  |
| `paint.gradient-matrix` |  |  |
| `primitives.fractional-geometry` |  |  |
| `regression.kitchen-sink` |  |  |
| `strokes.caps-joins-dashes` |  |  |
| `text.alignment-truncation` |  |  |
| `text.font-weight-style` |  |  |
| `text.mixed-style-runs` |  |  |
| `text.text-advanced` |  |  |
| `transform.nested-transform-stack` |  |  |
| `transform.transform-precision` |  |  |

## Commands

Run all scenarios:

```bash
cargo xtask parity_test --all --threshold 0.0
```

Run one scenario:

```bash
cargo xtask parity_test --scenario primitives.fractional-geometry --threshold 0.0
```
