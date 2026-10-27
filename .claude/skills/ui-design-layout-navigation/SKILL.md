---
name: ui-design-layout-navigation
description: "Use when laying out pages and app screens: grids, bento, heroes, sidebars, navigation, tables, tabs, cards and responsive structure."
---

# UI layout and navigation

Distilled from the STA-518 catalogue of UI design tips (YouTube creators @whosajid and kolejain). Treat as strong defaults, not law: apply what fits the product, and say when you depart from them.

## Rules of thumb

- Choose a layout pattern for what the content must do: bento grid when many features must be seen (vary tile sizes instead of uniform card rows), column grids for repeating content, tables for objects sharing properties, timelines for time-sequenced data.
- Treat the 12-column grid as a guideline. Align text hard left and prices hard right; keep rows at a readable width; repeat the section pattern for rhythm.
- Heroes: move off the centered stack, keep the focal point and backgrounds uncluttered, push decoration to edges, layer images behind smaller ones with margins around text.
- App shells: standard top actions + sidebar + content; left-align and tighten sidebars, move secondary links into popovers, drop the sidebar when navigation is simple, use a 3-4 link bottom bar on mobile, and flatten menus.
- One purpose per screen; keep primary actions in the same place on every screen; organize growing pages with tabs; avoid nested borders, containers and stacked radii.
- Group delivery/buy details with buy actions on product pages; serve cropped images to small screens; keep thumbnails at a fixed ratio.
- Remove elements that do not earn their place.

## Full list

`references/tips.md` holds every catalogued tip in this area with its rationale, grouped by source video. Read it when you need the reasoning or a specific case. The original catalogue is the `ui-design-tips-catalogue` document on STA-531. Sixteen @whosajid videos are not yet catalogued (YouTube IP block), so coverage is partial.

## In this repo

Studio's UI renders through Blitz (CLAUDE.md, "GUI rendering"), so where a tip assumes a browser, the `blitz-design` skill wins: inline styles only, no `position: fixed`, no `<dialog>`, `<details>`, `<datalist>`, `<progress>`, `<meter>` or `<select>`, no `placeholder`, no `text-overflow`, no CSS `fill`/`stroke` on SVG. Build those from `div`s, as the skill shows. Signal Touch's tokens and sizes (`app_shell::tokens`, `docs/signal/DESIGN.md`) are the palette and the scale; a tip about colour or spacing refines them rather than replacing them.

Copied from the fleet's Paperclip skill library (company Starcommand, `ui-design-layout-navigation`, revision 2, 2026-10-27). Paperclip holds the source: change it there and copy it again, so the two do not drift.
