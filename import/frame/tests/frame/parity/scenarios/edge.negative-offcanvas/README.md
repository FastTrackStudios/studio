# edge.negative-offcanvas

## What it tests
Nodes positioned partially or fully outside frame bounds.

## Setup
1. Create a Figma frame named `parity/edge/negative-offcanvas`.
2. Add these contents:
- Row A: partially off-canvas nodes.
- Row B: fully off-canvas but present in hierarchy.
- Row C: off-canvas nodes with effects/masks.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Culling and clip-space bugs commonly appear with negative/offscreen coordinates.
