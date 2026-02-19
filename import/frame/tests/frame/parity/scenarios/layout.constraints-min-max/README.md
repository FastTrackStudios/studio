# layout.constraints-min-max

## What it tests
Constraint anchoring and resize behavior under container changes.

## Setup
1. Create a Figma frame named `parity/layout/constraints-min-max`.
2. Add these contents:
- Row A: left/right/top/bottom pin examples.
- Row B: center and scale behaviors.
- Row C: min/max-like stress cases with clear labels.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Constraint logic is fundamental to responsive parity.
