# Two machines, one session: the DAW engine leads

Captured 2026-11-18, 06:58 (Pacific).
- **voyager** (macOS) runs Studio's engine, the `daw` extension, on **Drive To LA**, and its audio plays there.
- **THEBATTLESHIP** (Linux) runs Keyflow's chart, Ignition's cue list and Ignition's room. All three follow that engine.

This replaces the [earlier proof](../README.md), where Keyflow on voyager stood in for the engine.

## The setup

- **The org.** voyager hosts the org `acme`, with the issuer local to it. THEBATTLESHIP enrolled through `ARCHITECT_ENROL_AT`, as before.
- **The engine.** voyager's Architect loads `daw` (Studio's engine) with:
  - `FTS_DAW_OPEN=Songs/Drive To LA/Drive To LA.rpp`;
  - `FTS_DAW_BAR_ONE=5`;
  - `FTS_DAW_DOCUMENT=Songs/Drive To LA.session`.

  The engine opens the song's session as **the node's copy of a vault document**, and writes its timing into it: tempo, markers, sections, and bar 1 = session bar 5. It leads the transport (`lead/play`) on `0.0.0.0:4142`.
- **Who provides the session.** `EXT_SERVICE_USE {"service":"fts.session","provider":"daw"}` makes the engine the session's provider (`EXT_SERVICES`: `in_use: daw`, with providers `daw` and `keyflow`). voyager also runs the arrangement, the mixer and Keyflow's chart, all on that session.
- **What crosses the org.** The document `Songs/Drive To LA.session` (id `4d2107f2-…`) reaches THEBATTLESHIP through the org's sync, like any note. There, Keyflow opens it (`KEYFLOW_SESSION_DOCUMENT`). The cue list's bar 1 at 0:06.0 is the engine's bar 1, read from the synced document.
- **What THEBATTLESHIP follows.** It follows the engine's transport and clock at `ws://192.168.1.10:4142/vox`:
  - Keyflow's chart, through `KEYFLOW_SESSION_FOLLOW`;
  - Ignition's song and room, through `IGNITION_SESSION_SONG` and `IGNITION_SESSION_ROOM` (`--clock session:ws://192.168.1.10:4142/vox#4d2107f2-…@drive-to-la`).
- **Play and seek go to the engine alone** (`DAW_RUN SESSION_PLAY_FROM`, `SESSION_SEEK` and `SESSION_STOP` on voyager). Nobody else publishes a transport.

## The run

Play from 0, seek to 66 s (the Interlude), seek to 93 s (Pre-Chorus 2), stop.

- [voyager-run-window.mp4](voyager-run-window.mp4) is voyager's window for the whole run. It is 40 s long, cropped to the app window, with no sound.
- THEBATTLESHIP has stills only, cropped to its app window. The room is a process view, so these are screen grabs, not `APP_SHOT`s.

The engine's position (`PLAYHEAD_STATE`) and each chart's shown time (`KEYFLOW_SESSION_STATE`) were read one after the other. Each read is a separate CLI call, and THEBATTLESHIP's goes over SSH, so the later reads are a few tenths of a second behind:

| moment | engine, voyager | chart, voyager | chart, THEBATTLESHIP | voyager | THEBATTLESHIP |
|---|---|---|---|---|---|
| played from 0 | 8.25 s | 8.27 s | 8.42 s | [1](voyager-1-play-from-0.png) | [1](thebattleship-1-play-from-0.png): ▶ 0:07.2, cue 2 · Verse 1 (bar 1) |
| seek 66 s | 70.24 s | 70.26 s | 70.41 s | [2](voyager-2-seek-66s-interlude.png) | [2](thebattleship-2-seek-66s-interlude.png): ▶ 1:09.2, cue 7 · Interlude (bar 41); the room amber |
| seek 93 s | 97.25 s | 97.27 s | 97.42 s | [3](voyager-3-seek-93s-prechorus-2.png) | [3](thebattleship-3-seek-93s-prechorus-2.png): ▶ 1:36.2, cue 10 · Pre-Chorus 2 (bar 59) |
| stop | 93.00 s | 93.00 s | 93.00 s | [4](voyager-4-stopped.png) | [4](thebattleship-4-stopped.png) |

- **The clock.** THEBATTLESHIP's clock estimate of the engine's host had a 0.85 ms round trip.
- **The stop.** The engine returns to where it last started playing (93 s), and both charts follow it there.
- **The audio.** While it played, the mixer's meters (the engine's own values) moved on voyager; the Guide strip's meter is lit, at about −28, in still 3.

## What is not yet real

- **The transport does not cross the org.** THEBATTLESHIP dials the engine directly at `ws://192.168.1.10:4142/vox`, an address given to it by hand. Only the session *document* travels through the org. The transport and the clock are the engine's own WebSocket on the LAN.
- **The engine's vault-document mode is unmerged.** `FTS_DAW_DOCUMENT`, the engine holding its session as the node's copy so it syncs, is on Studio branch `feat/daw-session-vault-document`, which is local and not pushed yet (`7c2bc54e` is the code that ran, `ae0fe95e` the manifest). Studio `main` (`8fe8a836`) keeps the session in a file of its own beside the song, which no org syncs.
- **The engine adopted an existing document; it did not make the song's own.** `Songs/Drive To LA.session` is the document Keyflow made in the earlier proof. The engine wrote its timing into it.
  - The song's own document can't be made beside the `.rpp` yet. When the engine opens a song, Studio saves its own SQLite project as `<stem>.session`, the same name.
  - Architect has two traps here:
    - A kind keeps **one** file type: `DocKindDecl::to_doc_kind` takes `files.first()`.
    - The **last** extension to declare a kind id replaces the earlier declaration (`DocKinds::register`).

    So when `daw` declared `fts.session` with `files = ["fts-session"]`, every `.session` file stopped being a document of that kind for Keyflow too.
  - For this run, `daw` declares Keyflow's `session`.
  - This is to report to Architect and to settle with Keyflow: one kind, one file type, one place that declares it.
- **The cue list needed a hand.** Ignition's cue list, restored with the layout before `IGNITION_SESSION_ROOM` pointed it at the session, stayed on "■ 0:00.0" while the room followed. Closing that pane and opening it again (`WORKSPACE_CLOSE`, `EXT_VIEW_OPEN`) bound it. The setup does that now. It is for Ignition: the pane should rebind like the room does.
- **Keyflow's follow uses the session clock that session#189 removes.** Keyflow `main`'s `KEYFLOW_SESSION_FOLLOW` dials `session::clock`'s `SessionClockClient`. Studio's engine still serves it, because Studio pins session `84530a12`. When session#189 lands (`Leader` serves only Architect's presence, which carries its clock), Keyflow's follow has to read `PresencePeer::clock()` in the same move, or the chart stops following.
- **The audio was not heard or recorded here.** The engine plays to voyager's default output, the MacBook Pro speakers, and its meters move. The video has no sound.
- **The window chrome.** Both windows show the app's navigator sidebar: there is no action to hide it while the app runs (the launcher uses `dev --hide all`). THEBATTLESHIP's window is 1280×820.

## Builds

- **Architect:** `main` `48d3894e` (the v0.14.0 freeze) on both machines.
- **Studio:** `main` `8fe8a836` plus `feat/daw-session-vault-document` (above), on voyager only: `daw`, `studio.arrangement`, `studio.mixer`. Its session crate is session `84530a12`.
- **Keyflow:** `main` `bc59f0bb` on both machines; its manifest has `[uses] "fts.session"`.
- **Ignition:** `main` `628e70ad` on THEBATTLESHIP. The binaries were built from `0eb9903`, whose tree is `628e70ad`'s (it is the merge's second parent, with no diff). THEBATTLESHIP's root disk is full, so they weren't rebuilt.

The scripts that ran it are in [scripts/](scripts/): `run-a.sh` (voyager's node with the engine's environment), `setup-a.sh` (load, wait for the engine's session, the handshake), `setup-b.sh` (THEBATTLESHIP: the synced document, follow, the room, the cue list reopened), `round.sh` (this run) and `read.sh` (the readings).
