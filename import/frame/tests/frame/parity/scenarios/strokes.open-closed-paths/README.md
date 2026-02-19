# strokes.open-closed-paths

## What it tests
Differences between open and closed path rasterization with fill/stroke.

## Setup
1. Create a Figma frame named `parity/strokes/open-closed-paths`.
2. Add these contents:
- Row A: open paths with stroke only.
- Row B: closed paths with fill + stroke.
- Row C: self-intersecting path examples.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Path topology bugs can produce gaps, incorrect fills, or winding mismatches.
