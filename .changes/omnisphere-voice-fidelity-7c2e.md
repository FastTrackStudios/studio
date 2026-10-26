---
"synth" = "minor"
"sampler" = "patch"
---

Omnisphere patches sound closer to the plugin, measured against it: a soundsource's A440 tuning is divided out, a layer's Octave/Semi re-key the soundsource (the shifted note's zone plays), the Valve Radio effect is modelled (within ~1 dB), and a breakpoint filter envelope holds at its sustain point instead of its attack peak — `sustain` (an ADSR level, ranged 0..1) clamped the point index; it is now `sustain_point` (signal)
