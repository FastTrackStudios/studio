# effects.effect-order-stack

## What it tests
Effect ordering with fill, stroke, shadow, and blur combinations.

## Setup
1. Create a Figma frame named `parity/effects/effect-order-stack`.
2. Add these contents:
- Row A: fill + stroke + shadow.
- Row B: fill + blur + shadow.
- Row C: clip + blur + shadow + opacity stack.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Even with correct effects, wrong ordering causes visible mismatches.
