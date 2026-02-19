# edge.zero-size-nodes

## What it tests
Handling of zero-width or zero-height nodes.

## Setup
1. Create a Figma frame named `parity/edge/zero-size-nodes`.
2. Add these contents:
- Row A: primitive nodes with width=0 or height=0.
- Row B: zero-size nodes inside auto-layout.
- Row C: zero-size nodes with stroke/effect styles.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Zero-dimension objects often cause crashes or unexpected paint artifacts.
