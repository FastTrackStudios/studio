# The set engine

A set (a setlist, a show) plays in **one engine**: one `Standalone`, every
song of the set a project in it, one realtime audio engine on one device
mixing every song that is sounding. The engine is headless; the desktop
app, a browser, the CLI and models are its clients — on this machine or
another (`docs/design/one-engine.md`).

## What it must do

- **Every song sounds from the same callback.** Leaving a song's tab does
  not stop it; any number of songs can play at once.
- **Sample-accurate starts and stops.** A song starts on the exact frame
  another ends — the show plays through its songs seamlessly — because
  every song's transport takes its block plan from the same buffer clock
  and lands a `ScheduledLocate` (`transport_engine/block.rs`) inside the
  block, on its frame.
- **The engine owns the set.** Which songs are open, which is current,
  which are arriving, which are playing — engine state, changed through
  the Action List (`SESSION_SONGS`, `SESSION_SONG`, …) and published to
  every client. A window shows the set; it does not hold it.
- **Fast and steady opening.** The first song is playable first; the rest
  load on workers, never on a client's UI thread, and nothing a load does
  stalls a frame.
- **The split stays tied to the show.** A song cut from a show
  (`apps/session/daw/src/show.rs`) records where it sits in the show (the
  show file and its start, seconds). From that, the set is laid out on one
  show timeline (show timecode), and a song's edits can be written back
  into the combined session.

## Shape

```
engine process
  Standalone ── projects: song 1 … song n (each its own TransportShared)
  SetAudio ──── one output stream
                 per block: stamp = BufferClock.tick
                   for each voice (a song that may sound):
                     plan = voice.transport.begin_block(frames, stamp, period)
                     voice.renderer renders the plan's segments into scratch
                     scratch summed into the block
                   aux hooks (click, guide, reference) after
  Set ───────── songs, current, arriving, playing; actions; published
clients ── vox / Action List (MCP) ── the engine
```

- **Voices.** A voice is `(project guid, Arc<ProjectRenderer>,
  Arc<TransportShared>)`. Voices are added when a song opens (its renderer
  made and its snapshot built then — off the audio thread), and removed
  when it closes. The voice list is swapped in whole (no lock on the audio
  thread). A voice whose transport is stopped costs one `begin_block`.
- **Current** is what the window shows and where the transport keys go; it
  no longer decides what sounds.
- **Transitions.** `play_after(a, b)`: b's `ScheduledLocate { at: the
  moment a reaches its end, position: 0, playing: true }`, a's stop at the
  same moment — both in the sync clock, both landed on their frame.
- **Show timeline.** Each song's show offset gives every song an absolute
  position; a show transport maps show time to (song, song time), and the
  same scheduled locates play it.

## Steps

1. **`SetAudio` in daw-standalone**: one stream, a swappable voice list,
   per-voice block plans from one stamp, mixed; meters per voice; the
   existing aux hook after. Test: two projects in one engine, both
   playing, rendered offline through the same block loop, sum to both;
   one scheduled to start at frame N starts at exactly N.
2. **Session uses it**: `open::attach_audio` attaches the set's one engine
   once; opening a song adds its voice (snapshot built at open); switching
   songs changes only `current`. Fixes songs opened behind the first
   playing silence.
3. **The set moves into the engine**: the `Setlist` the window holds
   becomes a view of engine state; picks, opens and arrivals go through
   the actions; the window subscribes.
4. **Loading**: songs open on workers with the first one prioritised; a
   frame-time check fails when a load stalls a client frame; `session app
   open-setlist --bench` times request → first song playable → all local
   (target: JT's Senior Recital under one second from local disk).
5. **Show offsets and timecode**: `show.rs` writes each song's show offset;
   a show transport plays the set on the show timeline; edits written back
   into the combined session.
6. **Streaming a set to another client**: a second Studio or the web build
   opens the set from Task while the first serves it; the first song
   playable while the rest arrive.
