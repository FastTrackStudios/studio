# transform.nested-transform-stack

## What it tests
Hierarchical transform accumulation through parent/child chains.

## Setup
1. Create a Figma frame named `parity/transform/nested-transform-stack`.
2. Add these contents:
- Row A: parent rotate + child translate.
- Row B: parent scale + child rotate.
- Row C: 3-level transform chain.
3. Export bridge JSON to `export/latest-bridge-export.json`.
4. Export direct Figma PNG(s) to `expected/`.
5. Render your engine/plugin PNG(s) to `actual/` with matching filenames.

## Why this is tested
Matrix composition order must be exact for deep UI trees.
