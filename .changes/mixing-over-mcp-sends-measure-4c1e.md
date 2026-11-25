---
"studio-engine" = "minor"
---

Mixing over the MCP, by measurement: SENDS_LIST, SEND_ADD, SEND_SET and SEND_REMOVE (level in dB, pan, mute, post-fader / pre-fader / pre-FX, refused when it would feed back); SIGNAL_KIT `sends: true` makes the kit's routing (each send reported as there, missing or made); MIX_MEASURE renders a span offline through the whole mix — on copies of the song's plugins, Signal's in a session of their own, plugin delay compensated — and gives every track and bus its integrated and short-term loudness, true peak, RMS, crest, octave or third-octave bands, width and share of its folder, with `bypass_signal` the raw numbers beside them; MIX_RENDER writes the same span to a WAV; MIX_SNAPSHOT, MIX_RESTORE and MIX_DIFF keep, put back and compare whole mixes (faders, sends, FX and their CLAP states). A song's undo and going back no longer record another song's pending edits as their own (session)
