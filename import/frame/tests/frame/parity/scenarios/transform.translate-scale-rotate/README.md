# transform.translate-scale-rotate

## What it tests
Basic transform operations and origin handling.

## Setup
1. Create a Figma frame named `parity/transform/translate-scale-rotate`.
2. Add these contents:
- Row A: translation-only objects.
- Row B: uniform and non-uniform scaling.
- Row C: rotation around different origins.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Transform math errors quickly cascade to full-scene parity failures.
