---
name: ui-design-content-copy
description: "Use when writing or reviewing UI copy, hero imagery, pricing pages, empty states, social proof and landing page content."
---

# UI content and copy

Distilled from the STA-518 catalogue of UI design tips (YouTube creators @whosajid and kolejain). Treat as strong defaults, not law: apply what fits the product, and say when you depart from them.

## Rules of thumb

- Keep headlines and body short, specific and concrete; cut vague wordy copy and filler. Write friendly plain-language copy about how the product helps. Same destination means same button label.
- Use real product imagery (the actual dashboard) over stock; if stock, relate it to the product. Enlarge feature imagery and cut surrounding text. Match decoration playfulness to the product's tone.
- Pricing: trim to a manageable number of tiers, name plans for the customer's scale, show the actual discount and what the next plan adds, emphasize price over plan name.
- Empty states: use a full-screen first-use empty state pointing at the primary action; a no-results state needs imagery, acknowledgment, suggestions and an exit; show an empty state instead of empty controls.
- Lighter social proof, brand elements (badges, logo sets, proof links), 404 pages for personality.
- Do not explain a confusing UI with more labels or tooltips; simplify it. Design for dynamic content length, not just the static mockup.

## Full list

`references/tips.md` holds every catalogued tip in this area with its rationale, grouped by source video. Read it when you need the reasoning or a specific case. The original catalogue is the `ui-design-tips-catalogue` document on STA-531. Sixteen @whosajid videos are not yet catalogued (YouTube IP block), so coverage is partial.

## In this repo

Studio's UI renders through Blitz (CLAUDE.md, "GUI rendering"), so where a tip assumes a browser, the `blitz-design` skill wins: inline styles only, no `position: fixed`, no `<dialog>`, `<details>`, `<datalist>`, `<progress>`, `<meter>` or `<select>`, no `placeholder`, no `text-overflow`, no CSS `fill`/`stroke` on SVG. Build those from `div`s, as the skill shows. Signal Touch's tokens and sizes (`app_shell::tokens`, `docs/signal/DESIGN.md`) are the palette and the scale; a tip about colour or spacing refines them rather than replacing them.

Copied from the fleet's Paperclip skill library (company Starcommand, `ui-design-content-copy`, revision 2, 2026-10-27). Paperclip holds the source: change it there and copy it again, so the two do not drift.
