---
"session-engine" = "minor"
---

`DRUMS_TRIGGERS_FROM_MICS`: where a take's trigger recorded nothing for its drum (none of the piece's hit markers on that recording heard by the trigger, the close mics' heard), the trigger is made from the mics — a file in the song's own Media folder with a 1 ms click at each sure mic hit (the hits the kit's MIDI was made from) — and the takes cut from the dead file play it, so the drum's gate is keyed on the drum as played. `dry_run` says what it would do (session)
