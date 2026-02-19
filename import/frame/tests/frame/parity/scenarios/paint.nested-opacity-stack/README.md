# paint.nested-opacity-stack

## What it tests
Nested opacity multiplication and compositing order.

## Setup
1. Create a Figma frame named `parity/paint/nested-opacity-stack`.
2. Add these contents:
- Row A: single-group opacity variants.
- Row B: parent and child opacity combinations.
- Row C: opacity + blend mode interactions.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
This catches incorrect alpha propagation through scene hierarchy.
