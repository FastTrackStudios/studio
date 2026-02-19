# stress.large-canvas

## What it tests
Behavior on very large frame dimensions and wide coordinate ranges.

## Setup
1. Create a Figma frame named `parity/stress/large-canvas`.
2. Add these contents:
- Use frame size 4096x4096.
- Row A: sparse corner-placed content.
- Row B: dense center block.
- Row C: large gradients/images/vectors.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Large canvases reveal precision limits and memory/path planning issues.
