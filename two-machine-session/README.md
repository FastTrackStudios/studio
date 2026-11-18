# Two machines, one session, through the org

> **Superseded by [daw-engine/](daw-engine/README.md)** (the same day, 06:58): Studio's engine leads the transport, not Keyflow.

Captured 2026-11-18. Builds: Architect `main` `9d1f2f48`, Keyflow `main` `a283559` and Ignition `main` `a540496`. For 07–10, the fixes that are now merged were also installed (keyflow#110, keyflow#111, Ignition#24).

## The setup

- **voyager** (macOS) hosts the org `acme`, with the issuer local to it. **THEBATTLESHIP** (Linux) enrolled itself through `ARCHITECT_ENROL_AT`. Nobody set up a host by hand.
- `Songs/Drive To LA.session` reached THEBATTLESHIP as a vault document, an Architect document file of kind `fts.session`, synced by the org like any note. Each machine's Keyflow opened it from its own vault (`KEYFLOW_SESSION_DOCUMENT`).
- The chart's bar 1 (the Verse, 6.0 s in, after a count-in) and the tempo (160 BPM, 1.5 s a bar) come from the synced document.
- **The transport:** Keyflow on voyager publishes it (`KEYFLOW_SESSION_SERVE`, presence and clock only, not the DAW engine). On THEBATTLESHIP, Keyflow's chart, Ignition's cue list and Ignition's room follow it.

## The stills

| | voyager | THEBATTLESHIP |
|---|---|---|
| **Play, at 11.7 s.** 11.7 − 6.0 = 5.7 s = 3.8 bars: bar 4, beat 4 | [01](01-voyager-play-11.7s-bar4.png) chart in bar 4 | [02](02-thebattleship-play-11.7s-bar4.png) chart in bar 4; cue list on cue 2 · Verse 1 |
| **Seek to 60 s, 1.5 s later.** 55.5 s = 37 bars: bar 38, beat 1 | [03](03-voyager-seek-60s-bar38.png) chart in bar 38 | [04](04-thebattleship-seek-60s-bar38.png) chart in bar 38; cue list on cue 6 · Chorus lift (bar 37, 1:00.0) |
| Room at the chorus | | [05](05-thebattleship-room-seek-60s.png) |
| Cue list, zoomed, at 1:03.4 | | [06](06-thebattleship-cue-list-1m03s-cue6.png) |
| **voyager's app restarted mid-run, nothing re-run on THEBATTLESHIP**, then a seek to 60 s | [07](07-voyager-after-restart-seek-60s-bar38.png) chart in bar 38 | [08](08-thebattleship-after-voyager-restart-seek-60s.png) cue list on cue 6 at 1:01.8; [09](09-thebattleship-chart-after-voyager-restart-bar38.png) chart in bar 38; [10](10-thebattleship-room-after-voyager-restart.png) room at the chorus |

During 07–10, THEBATTLESHIP's room logged `no host yet; dialling until it answers` at 12:27:03, then `host answered` at 12:27:16. Its Keyflow and cue list had reconnected too, each holding a connection to voyager's transport.

01–06 are from the first pass on the mains, before the fixes. To get them, each window's restored chart and room pane had to be reopened by hand once; that is what keyflow#110 and Ignition#24 now do by themselves.

No video was recorded.
