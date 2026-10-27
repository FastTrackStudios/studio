---
name: ui-design-process
description: "Use when planning a UI design process or UX flow: reference collecting, wireframing, onboarding, undo/optimistic patterns, usability testing, and small web-delivery habits."
---

# UI design process and UX flow

Distilled from the STA-518 catalogue of UI design tips (YouTube creators @whosajid and kolejain). Treat as strong defaults, not law: apply what fits the product, and say when you depart from them.

## Rules of thumb

- Learn by copying: build a swipe file of inspiring sections, copy one exactly to learn its details, annotate its spacing/hierarchy/grouping, then restyle one layout in two flavors. Keep UI flavor separate from UX journey thinking; identity should come from what conveys the feeling, not a formula.
- Start with a wireframe stripped to structure; list repeating elements before designing and turn them into components under a shared design system.
- Map the user flow to shorten the path; design around the paths people actually take; give every action a visible response; give users a way out of onboarding; sequence onboarding one step at a time.
- Undo safety: Recently Deleted instead of final removal, immediate sent state with delayed send, version history for autosave, Escape cancels until commit, optimistic UI while saves run.
- Choose the interaction pattern by how sure the software is of intent. Settle design debates with usability tests against target users and competitors.
- Usability ahead of originality; attention to detail over heavy illustration or 3D. Small delivery habits: WebP images, favicon in the site root, correct mobile input modes.

## Full list

`references/tips.md` holds every catalogued tip in this area with its rationale, grouped by source video. Read it when you need the reasoning or a specific case. The original catalogue is the `ui-design-tips-catalogue` document on STA-531. Sixteen @whosajid videos are not yet catalogued (YouTube IP block), so coverage is partial.

## In this repo

Studio's UI renders through Blitz (CLAUDE.md, "GUI rendering"), so where a tip assumes a browser, the `blitz-design` skill wins: inline styles only, no `position: fixed`, no `<dialog>`, `<details>`, `<datalist>`, `<progress>`, `<meter>` or `<select>`, no `placeholder`, no `text-overflow`, no CSS `fill`/`stroke` on SVG. Build those from `div`s, as the skill shows. Signal Touch's tokens and sizes (`app_shell::tokens`, `docs/signal/DESIGN.md`) are the palette and the scale; a tip about colour or spacing refines them rather than replacing them.

Copied from the fleet's Paperclip skill library (company Starcommand, `ui-design-process`, revision 2, 2026-10-27). Paperclip holds the source: change it there and copy it again, so the two do not drift.
