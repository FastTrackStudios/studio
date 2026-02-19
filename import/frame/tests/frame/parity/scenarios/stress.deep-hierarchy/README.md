# stress.deep-hierarchy

## What it tests
Rendering stability and correctness in deep scene trees.

## Setup
1. Create a Figma frame named `parity/stress/deep-hierarchy`.
2. Add these contents:
- Row A: 10+ nested groups with visible content each level.
- Row B: nested clips and transforms.
- Row C: nested effects and opacities.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Deep hierarchies catch stack, inheritance, and traversal regressions.
