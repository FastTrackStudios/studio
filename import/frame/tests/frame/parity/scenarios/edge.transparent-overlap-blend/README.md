# edge.transparent-overlap-blend

## What it tests
Complex overlapping translucency with blending.

## Setup
1. Create a Figma frame named `parity/edge/transparent-overlap-blend`.
2. Add these contents:
- Row A: many semi-transparent overlaps.
- Row B: translucency + blend mode combinations.
- Row C: translucency + gradients + effects.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Premultiplied alpha and blend ordering issues are highly visible here.
