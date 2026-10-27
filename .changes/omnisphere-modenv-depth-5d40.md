---
"synth" = "patch"
---

Omnisphere's "Mod Env n Depth" rows play (2 505 rows, mostly velocity): measured, they cut the envelope's whole output as `clamp(1 + depth·source, 0, 1)` — a plain row does nothing, an inverted one scales the envelope down (velocity → depth, the common one) — exact against the plugin on a held envelope (signal)
