# strokes.boolean-ops

## What it tests
Boolean operation geometry correctness (union/subtract/intersect/exclude).

## Setup
1. Create a Figma frame named `parity/strokes/boolean-ops`.
2. Add these contents:
- Row A: each boolean op on simple shape pairs.
- Row B: nested boolean groups.
- Row C: boolean results with gradient/stroke styling.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Boolean path resolution is high-risk and frequently diverges from design tools.
