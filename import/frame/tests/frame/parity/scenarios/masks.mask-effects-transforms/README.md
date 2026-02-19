# masks.mask-effects-transforms

## What it tests
Mask behavior when combined with effects and transforms.

## Setup
1. Create a Figma frame named `parity/masks/mask-effects-transforms`.
2. Add these contents:
- Row A: masked node + drop shadow.
- Row B: masked node + blur.
- Row C: masked + rotated + blended stack.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Mask/effect/transform interactions are a common source of incorrect compositing order.
