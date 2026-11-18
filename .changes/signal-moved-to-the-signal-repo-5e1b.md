---
"app-shell" = "none"
"control-surfaces" = "none"
"session-desktop" = "none"
"session-engine" = "none"
"signal-web" = "none"
"song" = "none"
"studio-desktop" = "none"
"web-audio-worklet" = "none"
"xtask-workflows" = "none"
---

Changed nothing a user sees: Signal's crates (the domain, the rigs, the sampler, NAM and the nam, signal and sampler plugins) moved to FastTrackStudios/signal with their history and are taken from there by git; the rigs' private captures and frozen Cores come to the guitar crate's build from rig-library/; the shell's tokens and text-field hook are the rig frame's; the Komplete Kontrol driver is its own crate; Gig Performer's song import is song's example (signal)
