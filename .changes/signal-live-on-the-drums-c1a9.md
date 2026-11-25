---
"studio-engine" = "minor"
---

Signal live on a song's drums: FX_ADD takes a CLAP bundle (a path, or a bundle's name in ~/.clap or the CLAP search path) besides the FTS processors, and FX_SET / FX_PARAMS read and set it in its own words; FX_STATE and FX_STATE_SET read and load an FX's state, kept with the song; SIGNAL_KIT puts a Signal kit manifest's CLAP and per-track states on the tracks it names in one undo step; SIGNAL_BYPASS switches every Signal instance at once to A/B; FX_RELOAD swaps a rebuilt CLAP in under every instance of it, each keeping its state, while it plays. The engine hosts CLAP (daw-standalone's clap-host) (session)
