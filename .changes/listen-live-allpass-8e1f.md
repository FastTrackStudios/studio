---
"session-engine" = "minor"
---

LISTEN_LIVE opens a live tap on any point of a track (input, pre-fx, into or out of an FX, post-fx, post-fader) while the song plays — the audio thread only copies into a preallocated lock-free ring — and LISTEN_LIVE_READ measures the last few seconds as LISTEN does. LISTEN_ALIGN's phase correction is now the Signal align block's: up to two RBJ second-order all-pass sections `{hz, q}` and one first-order `{hz}`, flat in magnitude and without latency, reported as `allpass: [{hz, q}]` (session)
