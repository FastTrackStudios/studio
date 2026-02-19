# strokes.caps-joins-dashes

## What it tests
Stroke cap/join/dash semantics for vector outlines.

## Setup
1. Create a Figma frame named `parity/strokes/caps-joins-dashes`.
2. Add these contents:
- Row A: butt, round, square caps on identical line segments.
- Row B: miter, round, bevel joins on sharp angles.
- Row C: dash patterns with varying dash, gap, and offset.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Stroke parameter support is commonly incomplete and drives noticeable geometry differences.

## Artifacts

- Expected image: [expected/frame.png](./expected/frame.png)
- Actual image: [actual/frame.png](./actual/frame.png)
- Export JSON: [export/latest-bridge-export.json](./export/latest-bridge-export.json)
