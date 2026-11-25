# The engine through a link

**Status:** ported onto the new layout on `feat/web-engine` (studio): the
engine is the `daw` extension's browser build, linked into studio-web
(`studio_extensions::stage`), its views the arrangement, mixer, lyrics and
navigator extensions' own browser builds, the clock Architect's
`architect_playhead::engine` (architect `feat/engine-clock`). What a link
serves is opened; a member's own project from the server's files is not
built yet (the node serves no files to a browser but a link's).

The owner's ask: "We want to support multitracks and the full studio
engine through a link." A share link today plays each song's stereo
reference through Architect's media player, with the chart and lyrics
following. In addition a guest, or a signed-in member, opens a shared
setlist and gets the **engine**: every song's multitracks in sync, the
arrangement with its tracks, sections and markers, the mixer, the
Stems/Reference switch, and one transport the chart, lyrics and lighting
follow. In the web app, on a phone where the phone can carry it.

## 1. What runs in a browser today

| Piece | What it is | Engine |
| --- | --- | --- |
| `features/engine/daw-web` (`session-daw-web`, `just web-daw`) | Session's DAW view as a page of its own | **The real one**: one `daw_standalone::sync::Standalone` in the page (`studio_engine::web_engine`) |
| `features/adapters/web/worklet` | Signal's keys and guitar rigs in an `AudioWorklet` | A `Standalone` inside the worklet, for the rig only |
| `apps/architect/island` (`studio.transport`) | The share link's views (playlist, chart, lyrics, a section ruler) in Architect's sandboxed frames | None: it reads the page's playhead and asks `PLAYHEAD_*` |
| `features/engine/web`, `apps/studio/web` | Sites | None |

So a browser engine exists and is the desktop's engine, not a second one.
What it does, measured from the code (`features/engine/runtime`,
`features/engine/engine/src/web_engine.rs`):

- **A set in one engine.** Every song is a project in the page's one
  `Standalone`; `switch_song` moves the audio between them. The first
  song opens first, the rest behind it (four at a time).
- **Where audio is made.** The mix is rendered on the page's main thread
  (`ProjectRenderer::render_block`, 1024 frames, every 10 ms and on the
  worklet's own messages) and posted to a small worklet (`fts-out`) that
  only plays a queue: 0.25 s ahead in front, 2 s when the tab is hidden.
  No `SharedArrayBuffer`, no threads, so no COOP/COEP is needed. Fixed
  48 kHz; the browser resamples to the device. Audio starts on the first
  press (a browser's rule).
- **Formats.** Ogg Vorbis only (`fts_sample::ogg_stream`, pure Rust
  symphonia). Masters are never played in a page.
- **Streaming.** A rendition is never downloaded whole before it plays.
  Each has a page index beside it (`<name>.ogg.idx`, about a point a
  second), so a take is attached as sparse bytes
  (`attach_remote_ogg` over `SparseBytes`) and one fetcher per song
  brings ranges in the order they will be heard
  (`media_fetch::drive`: 3 s, 10 s, 30 s, 120 s, then the rest; eight
  requests at once; 256 KB blocks kept in the browser's Cache Storage).
- **Memory.** Compressed bytes are held for a window around the playhead
  (`Resident::BROWSER`: 5 s behind, 30 s ahead of each take). Decoded
  audio is kept as 16-bit in 16 384-frame chunks, eight chunks ahead:
  about 1.1 MB a stereo stem. Nothing is ever decoded whole. There is no
  track cap; the limit is the decode budget on the page's thread (6 ms a
  tick, 24 ms catching up after a jump).
- **Reference mode.** A song with a reference bounce
  (`Media/Proxies/Reference.ogg`, and a mono 24 kHz preview) is heard by
  that one stream; its stems are attached and drawn but not fetched until
  they are wanted, and take over once they have arrived at the playhead.
  A host can cap the page at the reference (`Listen::Reference`).
- **Views.** `session-ui`'s `web` feature draws the same Arrangement and
  Mixer widgets as the desktop into WebGL2 canvases (`web_host`), with a
  small-screen layout (`compact`).

**What was missing for a shared set:**

1. Every source was Task's (`ShareSource`, the Task live set). Nothing
   opened a set from an Architect share link.
2. The engine was a page of its own. In Architect's web app the only
   Studio code was the island, whose frame can reach nothing
   (`connect-src 'none'; media-src 'none'; worker-src 'none'`): it cannot
   fetch a range or run an engine, so the shell's Mixer tab was a
   placeholder and its Arrangement a section ruler over the playhead.
3. Two clocks: Architect's playhead (an `<audio>` element or its own Ogg
   decoder) and the engine's transport knew nothing of each other.
4. No rendition existed for the recital set (masters only), and nothing
   said which layer of a link a session's files are in.
5. A set is saved in whichever way it was last heard on the desk
   (`SESSION_HEAR`): heard by its reference recording, every instrument
   folder is muted, and it opens silent anywhere else.

## 2. What is built

```
 share link  /s/<code>/…                      Architect's node (unchanged)
   ├─ the page ──────────► Studio's build of Architect's web app
   │                        (extensions/daw (its browser build): architect_web::run)
   ├─ _files.json ───────► LinkSet: the set, its songs, the link's layer
   ├─ <Song>.session ────► one Standalone, a project per song
   └─ …/Proxies/*.ogg ───► ranged reads, in the order they will be heard
        (Range: bytes=…)        │
                                ▼
            engine transport ═══ architect.playhead's clock (kind `engine`)
                                │
        chart · lyrics · lighting (the island's views) follow the playhead
        Arrangement · Mixer tabs: the engine's own panels, in the page
```

### 2.1 A set from a link (`engine_runtime::link`)

`LinkSet::open(link)` reads the link's own listing,
`<link>_files.json` (Architect's `files_json`): its notes in list order,
each with the files it attaches, and each file with its size, layer and
URL. A **song** is a note that attaches a project file (`.session`, else
`.RPP`); its folder is that file's; its files are the link's files under
that folder. `LinkSong` is a `SongSource`, so the rest is what already
streamed a Task song: the small files mirrored into memory, each
rendition read by `Range` from its own URL on the link. Nothing here
knows where the node holds a file.

The node has already left out every file above the link's layer, so what
is listed is what this listener may fetch. The listing's `listen` is the
cap the engine opens under: a link that plays to `reference` opens the
arrangement and the reference and is never offered the multitracks.

`web_engine::WebSource::Link { url }` opens that set as the page's
engine. Each song is set to its stems as it opens
(`engine_runtime::reference::hear`, moved out of the desktop's
`SESSION_HEAR` so both use one rule), in the page's engine only.

### 2.2 The vault, the rendition and where it is made

A song's folder in the vault:

```
Songs/<Song>/<Song>.md                       type: song; attaches the files below
Songs/<Song>/<Song>.session                  the project, its log folded in
Songs/<Song>/…/Proxies/<stem>.ogg (+ .idx)   one rendition a recording
Songs/<Song>/Media/Proxies/Reference.ogg     the reference bounce (+ preview, indexes)
```

- **Codec and bitrate: Ogg Vorbis, libvorbis quality 0.4** (about
  128 kbps stereo, VBR), at the master's sample rate. It is the one codec
  the engine decodes on every target (pure Rust, no `AudioDecoder`, so an
  iPhone before iOS 18.4 plays it too), it seeks by page with a small
  index, and it is what Task's links already stream. Opus would be about
  a third smaller at the same quality but has no decoder in the engine
  yet; AAC needs the browser's decoder, which cannot be driven sample
  accurately across 35 streams.
- **Where it is made:** on the machine that holds the masters, when the
  song is prepared for a link (`link-prepare`, a bin of `studio-engine`
  over `ingest::prepare`, what `studio ingest` runs before an upload):
  the project's log folded in, a rendition and page index a recording,
  the waveforms (and a coarse copy of each for the link, 80 KB a track,
  in `Proxies/Peaks/`), and the reference bounce. A step that is current
  is skipped, so a second run does nothing.
- **Where it is stored:** beside its master, in a `Proxies/` folder, as a
  file of the vault. **It is an attachment like any other:** the song
  note links it in its body (a note's attachments are the files its body
  links), the vault syncs it, and a link serves it from wherever
  it is held. `scripts/link-vault.py` builds such a vault from prepared
  song folders.
- **Layers** (`features/setlists/extension.toml`, `[[share_layers]]` for
  `song`): the project file and the reference bounce are the
  **Reference** layer's; each track's rendition and its index are the
  **Full** layer's. The masters are not attached to the note, so they are
  on no link at all.
- **The master, only when a member asks:** not built. A member's "play
  the masters" is `fetch_original` on desktop (a WAV, memory-mapped); a
  page has no disk to map one from, so in the web app it would have to be
  a second, lossless rendition (FLAC) the engine cannot decode by range
  today. Stubbed; see §7.

The reference a link's listener hears is the one the desk's Reference
mode plays: the song's own reference recording (its top-level `Reference`
folder alone, `reference::bounce_for_link`), and the stems' mix for a
song with none, or whose recording does not decode (a video's audio).

### 2.3 Fast start, bounded memory

Nothing new was needed in the engine: §1's streaming and windows apply to
a link's set as they are. What a link adds is the order of work on the
page: the listing (one request), the first song's small files, the first
song's opening seconds of each rendition, then the rest of the set behind
it. The numbers are in §6.

**Reference mode as the fallback.** `web_engine::carry()` reads what the
device says of itself: fewer than four processors, less than 4 GB
(`navigator.deviceMemory`, where a browser says), or Save-Data, and the
set is heard by its references: no stem is fetched behind them. The page
says so in the view ("Reference mix: this device was judged too small for
the multitracks (N MB). Tap to load them anyway."), and the tap, or a
change to a stem's level, offers the multitracks
(`load_multitracks`), remembered for that browser. A link that plays only
to `reference` says that instead and offers nothing.

### 2.4 One clock

Architect's playhead is the transport everything follows. Its player is a
`Clock` (kind `media`). Studio's web build gives it a second one, the
engine (kind `engine`, `extensions/daw (its browser build)/src/clock.rs`):

- An item is the engine's when the engine has open the song whose folder
  holds the item's media. For such an item the app loads, plays, seeks
  and reads the **engine**; its `<audio>` element and decoder hold
  nothing. One thing is heard.
- An item the engine does not carry (a song still opening, a note that is
  not a song, an engine that failed to open) plays on the app's player as
  before. When the engine opens, or another song arrives, the app asks
  again whose the current item is and the engine takes it over where the
  deck is.
- The deck's time is the engine's while it moves, so the chart, the
  lyrics and the lighting follow what is heard, sample for sample with
  the stems.
- A press in the engine's own panels moves the engine first; a watcher
  hands it to the deck as the app's own press, and a song picked there is
  the deck's `PLAYHEAD_ITEM`.

### 2.5 The views

The shell's tabs are the island's (`[[favourites]]` for a setlist: More →
Arrangement, Mixer). Where the app would mount the island's frame for
`studio.transport/arrangement` and `studio.transport/mixer`, Studio's web
build draws the engine's own panels in the page
(`session_ui::web_host::HostedView`, the widgets the desktop extensions
`extensions/arrangement` and `extensions/mixer` draw, hosted in a canvas).
The island's ruler and its "not here" notes remain what a build without
the engine shows.

**On a phone** (390 px): the tab is one panel, full width, under a
one-line head (the song, how it is heard). The Arrangement is the track
list at its narrow width beside the timeline, scrolled by a drag and
zoomed by a pinch, the transport being the shell's own Now Playing bar;
the Mixer is the strips alone, scrolled sideways. The engine's own small
layout (`compact`) is not used here: the shell is Architect's.

## 3. What a guest can change

- **Mute, solo, levels, pans: the listener's own.** They are edits to the
  page's own engine. They are not written back because there is nowhere
  to write them: every route of a link the engine uses is a read
  (`GET` with or without `Range`), the page joins no session
  (no `collab::join_task`), and it keeps nothing (`Keep::Memory`; only
  rendition bytes go to the browser's cache). A reload starts from the
  link's files again.
- **Read-only otherwise.** The set cannot be reordered or have songs
  removed (`ReadOnlySet`). Edits to the arrangement itself are not yet
  refused in the page; they would change the page's copy only and are
  lost on reload. A link that grants more is a permission of the link
  (§5), not another view.

## 4. Running it

```sh
# 1. Renditions, on a copy (the masters are read, never changed):
cargo build -p studio-engine --bin link-prepare
target/debug/link-prepare /abs/copy/of/song-folder …
# 2. A vault whose song notes attach them:
python3 -I scripts/link-vault.py /abs/copy /abs/vault --setlist "/abs/copy/Set.setlist"
# 3. The stack on it, the link playing to Full:
LOCAL_STACK_VAULT=/abs/vault LOCAL_STACK_SETLIST="Setlists/Set.md" \
LOCAL_STACK_LISTEN=Full LOCAL_STACK_WEB=app \
LOCAL_STACK_OVERRIDES=… LOCAL_STACK_ARCHITECT_PATH=… just demo
```

`LOCAL_STACK_WEB=app` serves the web app alone (no site, no docs).
`session-daw-web` opens the same link as a page of its own:
`/session/?link=<link>`.

## 5. The contract needed from Architect

Built on my Architect branch, as the narrowest thing that works; each is
a request to the owner of that code, not a decision:

1. **A distribution's web build gives the app its own views and an engine
   clock** (`apps/web/src/provided.rs`: `provide_view`, `provide_engine`,
   `EngineClock`). A guest runs no linked extension (#805), so this is the
   narrow form of it for a distribution's compiled-in views. It goes when
   a guest's host loads the distribution's linked extensions under
   `share_needs::guest_capabilities`; the engine clock stays.
2. **`<link>_files.json` stays as it is** (path, url, size, sha256, layer,
   note; notes with their files; `listen` and `max`), and every listed
   file answers `Range` with `206`. This is all the engine uses.

Wanted, not built (the file-delivery thread's, `feat/link-files-from-any-holder`):

3. **A note attaches a folder.** A song is a hundred files (a rendition,
   an index and a waveform a track). Today the note links each in its
   body, where a guest reads them as a list of file names. One
   attachment that is a folder, listed file by file in `_files.json`,
   would make a song's session one line.
4. **"Plays, is not offered".** A link serves a rendition to the engine
   and must not list it on the files page or in "Download all". Today the
   layer decides both. Wanted: a per-file flag in the listing
   (`"offer": false`) the files page and the archive honour, set by the
   same `[[share_layers]]` rule (`play = […]`).
5. **Immutable by hash.** A URL that carries the file's sha256 (or an
   `ETag` of it with `Cache-Control: immutable`) so a rendition fetched
   once is never asked for again, across links and sessions. Today the
   engine keys its cache by path.
6. **A batch of small files.** A song's documents are a handful of
   requests; a set of sixteen is a hundred. One `POST` answering many
   small files (as Task's `docs`) would take a set's open from round
   trips to one.
7. **Bytes from any holder.** Whatever serves the file (the node's disk, a
   bucket, a storage server), the URL in the listing is what the engine
   reads by range; a redirect to the holder is fine if it keeps `Range`
   and CORS for the link's origin.

## 6. Measurements

Taken on 2026-11-18 on THEBATTLESHIP with `scripts/engine-link-probe.mjs`:
Firefox in a user and mount namespace of its own (this machine's audio
sockets and sound cards hidden), playing into a null sink of a PipeWire
the probe starts and records. The recital set, the stack's node on
loopback. **The machine was saturated by other builds throughout (load
average 27 to 165 on 32 cores), so every time below is an upper bound and
the real-time results are not a verdict on an idle device.**

The set: 16 songs, 261 track renditions and a reference a song. 15.9 GB
of masters became 536 MB on the link (Ogg Vorbis q0.4, waveforms
included); "Hey!" (35 tracks) is 2.51 GB of masters and 50 MB on the
link, 3.9 MB of it small files. A link that plays to Reference serves
83 MB for the whole set.

"Hey!", 35 tracks, the largest song:

| | Reference layer, 1280 px | Full layer, 1280 px | Full layer, 390 px |
| --- | ---: | ---: | ---: |
| The engine has the link's first song open | 4.7 s | 3.4 s | 1.9 s |
| First sound after the press | 3.0 s | 8.6 s | 9.2 s |
| 30 s of wall clock: transport advanced | 17.2 s of 20 | 12.3 s | 8.1 s |
| Frames that were not silence | 17.6 s of 20 | 12.6 s | 11.5 s |
| Underrun frames | 0 | 5.9 s | 12.1 s |
| At the null sink (sox): peak, RMS of the last 5 s | not recorded | 0.39, 0.056 | 0.39, 0.040 |
| The tab's process, resident | 919 MB | 1.79 GB | 1.80 GB |
| The same browser on a blank page | 407 MB | 407 MB | 422 MB |

Other songs on the Full layer at 1280 px, under heavier load (115 to 165):
"Cinematic Intro" (18 tracks, the set's first song: first sound 2.5 s
after the press, 2.1 s of transport in 30 s) and "Sleeptalker" (7 tracks:
first sound 5.4 s, 3.8 s of transport in 30 s).

What the numbers say:

- **The Reference layer plays.** One stream, in step, no underruns, with
  the machine at load 111.
- **The stems did not hold real time in these runs.** A 7-track song fell
  behind as far as a 35-track one, so the cost is not the number of stems:
  each of these projects is the recital's whole template (179 tracks),
  and the page mixes all of it on its own thread. Whether an idle desktop
  holds it was not measured. Until it is, a link's listener should be
  taken to the reference by default more readily than `carry()` does now
  (it judged this browser able), and the mix belongs in a worker.
- **Memory is not yet bounded tightly enough for a phone.** About 0.7 GB
  over a blank page for sixteen open projects before anything plays, and
  1.4 GB with a song's stems playing. The streamed audio is the small
  part of that (§1); the projects themselves and the 41 MB module are
  the rest. Opening only the song on show, and the others when picked,
  is the next cut.
- Two things found here and fixed: with no waveform on the link the page
  decoded every rendition whole to draw it (first sound went from 23 s to
  8 s once `link-prepare` wrote coarse waveforms), and a build without
  SIMD mixed ten times slower.

## 7. Stubbed or not built

- The master for a member who asks for it (§2.2).
- Going back from the stems to the reference without a reload: the switch
  is one way in a page (reference → stems). The desk's `SESSION_HEAR`
  goes both ways.
- Arrangement edits are not refused for a guest (§3).
- The page's build has no place in a release yet: `LOCAL_STACK_WEB=app`
  serves it from the stack, and `scripts/studio-architect.sh web` still
  bundles Architect's own wasm.
- Every song of the set is opened into the page's engine up front; only
  the song on show streams, and the other songs' stems are not
  pre-fetched on a link.
- Lighting follows the playhead as before; the engine does not drive
  Ignition in a browser.
- A signed-in member opening a project from their own vault (not a link)
  gets the app's player, not the engine: `WebSource::Link` is a link's.

## 8. Verified, and not

Verified on the local stack (issuer, node and web front from this tree
and the Architect branch, ports 7520 to 7523):

- The link lists the set (844 files: 499 Full, 80 Reference, the
  waveforms as page files on the node that ran, which predates their
  rule) and answers a `Range` with `206`; at `?listen=reference` it lists
  no rendition.
- The web app opens as the link's guest, the engine opens the link's
  first song in 2 to 5 s and all sixteen behind it, and it is the
  playhead's clock (`data-engine-clock="engine"`): a press on a song in
  the island's playlist plays it through the engine.
- Sound was measured, not inferred: at the engine's worklet and, on the
  Full layer, at the null sink it played into (§6).

Not verified:

- **The Arrangement and Mixer were never seen drawn.** The sandboxed
  Firefox offers no WebGL2 on this machine (headless, or under Xvfb,
  where the probe's browser stopped answering), so the tabs showed the
  views' own "Hardware acceleration needed" notice. They compile and
  mount; nothing more is claimed. Mute, solo and levels were therefore
  not exercised in a browser either.
- No real audio device was opened, on purpose (the machine's interface is
  the live monitoring path).
- A phone was not used: 390 px is a desktop Firefox at that width.
- `just dev-smoke` was not run; its checks of the island's Arrangement
  frame no longer apply to a link with a session, and it was not updated.
