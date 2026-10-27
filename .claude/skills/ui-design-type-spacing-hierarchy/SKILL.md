---
name: ui-design-type-spacing-hierarchy
description: "Use when designing or reviewing UI typography, spacing and visual hierarchy: font choice and sizes, line height, 4-pt spacing, proximity grouping, emphasis and button ranking."
---

# UI type, spacing and hierarchy

Distilled from the STA-518 catalogue of UI design tips (YouTube creators @whosajid and kolejain). Treat as strong defaults, not law: apply what fits the product, and say when you depart from them.

## Rules of thumb

- Use one sans-serif family (optionally a serif or a characterful display face for headlines, with a neutral body face). Build hierarchy from a small set of sizes, then let weight and color do the rest.
- Pick one base font size and deviate only by a few px. Define type styles (size + line height per role) once as variables.
- Tighten letter spacing and line height on large headings; loosen slightly for body. Lighten weight when a bold heading feels harsh.
- Step text tone down from headings to body to subtext by lowering lightness, not by an arbitrary gray.
- Keep dashboard text small; do not shrink type and spacing below desktop scale on other devices.
- Spacing: base everything on a 4-pt grid and one consistent scale. Group related elements by proximity; use generous white space; avoid cards inside cards (group with space instead). Let line height supply spacing under text.
- Hierarchy: make the most important item large and bold, create emphasis by contrast with neighbors, and use size, color and position together. Scale button tone from ghost to primary; make the best option the most obvious.
- Hide infrequent actions in popovers, reveal row actions on hover, and ask of every element whether it earns its place before adding color or effects.
- Check type by zooming out and comparing with neighboring elements.

## Full list

`references/tips.md` holds every catalogued tip in this area with its rationale, grouped by source video. Read it when you need the reasoning or a specific case. The original catalogue is the `ui-design-tips-catalogue` document on STA-531. Sixteen @whosajid videos are not yet catalogued (YouTube IP block), so coverage is partial.

## In this repo

Studio's UI renders through Blitz (CLAUDE.md, "GUI rendering"), so where a tip assumes a browser, the `blitz-design` skill wins: inline styles only, no `position: fixed`, no `<dialog>`, `<details>`, `<datalist>`, `<progress>`, `<meter>` or `<select>`, no `placeholder`, no `text-overflow`, no CSS `fill`/`stroke` on SVG. Build those from `div`s, as the skill shows. Signal Touch's tokens and sizes (`app_shell::tokens`, `docs/signal/DESIGN.md`) are the palette and the scale; a tip about colour or spacing refines them rather than replacing them.

Copied from the fleet's Paperclip skill library (company Starcommand, `ui-design-type-spacing-hierarchy`, revision 2, 2026-10-27). Paperclip holds the source: change it there and copy it again, so the two do not drift.
