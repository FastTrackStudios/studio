---
"synth" = "patch"
"sampler" = "minor"
---

Omnisphere's amp-envelope timing plays per note: the Speed knob (`spd`) scales every segment by a measured law (×2.49 slower … ×0.1 faster), Key and Velocity rows onto `AmpEnv Speed` (1 929 rows) move that knob note by note, and a Velocity row onto `AmpEnvAtk` (1 524 rows) shortens the attack by `1 − depth·velocity²` — on synth and sample layers, within a few % of the plugin's attack and release times. An envelope player takes a per-note time scale (signal)
