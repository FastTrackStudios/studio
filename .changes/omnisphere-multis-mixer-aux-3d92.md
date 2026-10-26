---
"synth" = "patch"
"daw-standalone" = "none"
---

Omnisphere multis play through their mixer and racks: the part fader follows the measured layer-level taper (it was a plain 20·log10, ~3 dB off at the factory's usual settings), part pan applies, each part's sends feed the four multi aux racks (`AEffRack0..3`) at the part aux law, and the master rack (`MEffRack`) closes the chain; `omni_render` and `render_patch` take a `.mlt_omn` so multis A/B like patches (signal)
