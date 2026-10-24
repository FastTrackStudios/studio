# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project follows [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).
Each release's section is built by `just release-prepare` (scripts/release.sh)
from the cargo-rail change files in `.changes/` (up to 0.1.2, by `git cliff`
from the commits since the previous release); edit it only in a release PR.

## [26.10.2] - 2026-10-24

_The first CalVer release to ship. 26.10.0 and 26.10.1 merged but were never tagged or shipped: everything in them ships here._

### New

- Studio for the desktop: a signed, notarized macOS app (Apple silicon) and a Linux AppImage (x86_64), attached to this release (studio)
- One command installs or updates it: `curl -fsSL https://raw.githubusercontent.com/FastTrackStudios/studio/main/scripts/install.sh | sh` (studio)
- Studio for iPhone and iPad ships to the TestFlight beta from the same release (studio)
- The Signal browser remote builds again (signal)
- The Linux AppImage needs glibc 2.42 or newer for now (Arch, Fedora 43, Ubuntu 25.10+); a build for the LTS distros is next (studio)

### Fixed and improved

- A rebuilt plugin no longer allocates on the audio thread on its first render on macOS (session)
- Document mode plays its legato transitions and its off-grid shorts on time again, starting in the right articulation; libraries with a legato engine but no vibrato controller (Cinematic Brass) now play legato transitions live; a note-on no longer allocates on the audio thread (signal)
- Every chorus and vibrato space, ce2 among them, shows its own badge in the rail (studio)
- In the browser rig, a pedal saved as off plays its model when stomped on, seamlessly even when the worker falls behind (signal)
- PipeWire MIDI delivered as UMP plays: words are read in native byte order (every UMP note was dropped), and the UMP control type is matched by its ABI value, so the crate builds against PipeWire before 1.4 (Ubuntu 24.04) (signal)
- PSOLA pitch detection no longer allocates on the audio thread and runs about ten times faster (studio)
- Stretched audio plays the run-up to each hit as recorded, so quantized hits land on the grid instead of 2–3 ms early (session)
- The gate visualiser's threshold readout is a well-formed control (studio)
- The maximal template's Compress and FX checks find Process under Drums (session)
- The spectral bell reduces only inside its band again, not everything above it (studio)
- The Velvet reverb no longer allocates on the audio thread (studio)


## [26.10.1] - 2026-10-24

_26.10.0 merged but was never tagged or shipped: its changes ship in this release._

### New

- Studio for the desktop: a signed, notarized macOS app (Apple silicon) and a Linux AppImage (x86_64), attached to this release (studio)
- One command installs or updates it: `curl -fsSL https://raw.githubusercontent.com/FastTrackStudios/studio/main/scripts/install.sh | sh` (studio)
- Studio for iPhone and iPad ships to the TestFlight beta from the same release (studio)
- The Signal browser remote builds again (signal)

### Fixed and improved

- A rebuilt plugin no longer allocates on the audio thread on its first render on macOS (session)
- Document mode plays its legato transitions and its off-grid shorts on time again, starting in the right articulation; libraries with a legato engine but no vibrato controller (Cinematic Brass) now play legato transitions live; a note-on no longer allocates on the audio thread (signal)
- Every chorus and vibrato space, ce2 among them, shows its own badge in the rail (studio)
- In the browser rig, a pedal saved as off plays its model when stomped on, seamlessly even when the worker falls behind (signal)
- PipeWire MIDI delivered as UMP plays: words are read in native byte order (every UMP note was dropped), and the UMP control type is matched by its ABI value, so the crate builds against PipeWire before 1.4 (Ubuntu 24.04) (signal)
- PSOLA pitch detection no longer allocates on the audio thread and runs about ten times faster (studio)
- Stretched audio plays the run-up to each hit as recorded, so quantized hits land on the grid instead of 2–3 ms early (session)
- The gate visualiser's threshold readout is a well-formed control (studio)
- The maximal template's Compress and FX checks find Process under Drums (session)
- The spectral bell reduces only inside its band again, not everything above it (studio)
- The Velvet reverb no longer allocates on the audio thread (studio)

## [26.10.0] - 2026-10-23

### Added

- Signal's 43 commands are one architect Action List: keys, MIDI, REAPER and a model run them by id. `studio-desktop --engine --mcp` (or `SIGNAL_MCP=1`) serves them over MCP on loopback, behind a token it generates and writes to `~/.config/fts/signal-mcp.json`.
- Added Studio, one app for Signal and Session: its front screen opens either one, it has a new icon, and it ships to TestFlight as Studio in place of the separate Signal and Session apps (studio)

## [0.1.2] - 2026-10-22

### Fixes

- *(release)* One GitHub Release per tag when publish and both ios-release steps race ([#31](https://github.com/FastTrackStudios/studio/pull/31))
- *(session-ios)* The iOS release build compiles reaper-embed and finds the repo root ([#32](https://github.com/FastTrackStudios/studio/pull/32))
- *(session-ios)* Daw-standalone's MIDI input route is inert where midi has no hub (iOS) ([#34](https://github.com/FastTrackStudios/studio/pull/34))
- *(release)* Cargo.lock as cargo regenerates it (v0.1.1's lock bumped crates that own their version) ([#36](https://github.com/FastTrackStudios/studio/pull/36))
- *(signal/content-client)* Refuse host paths that leave the libraries dir ([#33](https://github.com/FastTrackStudios/studio/pull/33))

## [0.1.1] - 2026-10-22

### Build

- *(ios)* Link the rig captures (rig-models) on the airlock runner before the release build (STA-82) ([#21](https://github.com/FastTrackStudios/studio/pull/21))
- Read-only token for the private inferno-control git dep ([#6](https://github.com/FastTrackStudios/studio/pull/6))
- *(pr-base)* Retarget PRs from main to dev; main stays the default branch ([#24](https://github.com/FastTrackStudios/studio/pull/24))
- *(ios)* Fetch rig-models before linking captures; just assets over https (STA-312) ([#25](https://github.com/FastTrackStudios/studio/pull/25))
- *(pr-base)* Leave the dev -> main promotion PR on main ([#27](https://github.com/FastTrackStudios/studio/pull/27))

## [0.1.0] - 2026-10-21

### Features

- *(signal)* All-Around default profile; scenes inherit by reference with per-song overrides ([#10](https://github.com/FastTrackStudios/studio/pull/10))

### Changes

- Session, daw, processor, signal and patchbay in one repository
- Carry over session's and daw's commits since the cut
- The rigs' captures move to the rig-models repo
- One workspace: daw, processor, signal and patchbay join the root
- Take architect's action-list branch from GitHub, not a local checkout
- Frame joins studio, its two branches merged first
- X86-64-v3 on every x86-64 desktop build
- Every crate in its place: apps/, crates/, features/, vendor/, xtask/
- The agreed tree: crates/ is daw, frame, keyflow; everything else a feature
- One crate (gate-dsp's code at its root, the facade's dsp alias kept)
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- Delay, reverb: their editors keep viz where the workspace gave it
- Eq-standalone is an app: apps/plugins/eq-standalone
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- One crate
- Every folded crate builds on its own, not only in the workspace
- One crate
- Repairs to the folds: comments, a lost feature, gate implications
- One crate
- One crate
- One crate
- Three crates to where they belong before their groups fold
- One crate
- One crate
- Guide.rs → guide/mod.rs (room for its engine)
- One crate (with the guide's engine at session::guide::engine)
- One crate (12 folded)
- The plugin glue out of audio
- Workspace tests build every folded module; formats' fixtures by format
- The standalone bin's path after the fold
- Gated targets: the launcher bin names its crate; rig-host's quantum request per jack feature
- The backends are about to be its modules; drop the facade's inline re-exports
- One crate
- One crate
- Comp, delay, reverb: dsp-core's UI features only behind ui/native
- The browser rigs' AudioWorklet entry, one crate
- What the ui modules ask of shared dependencies stays behind ui
- Drop the facade crate (nothing used it); frame is about to be one crate
- One crate (31 folded), its tools inside it
- Name anyrender 0.11's git source directly, so dx can read the workspace
- The app and its browser remote are one crate
- The signal CLI and fts-convert are its binaries
- Session, session-player and session-vault-sync are its binaries
- Name the features the app CLIs and worklet rigs sit behind
- The umbrella crate; daw is the DAW alone
- No REAPER beneath the sync engine
- Daw-transport-sync is the core's (crates/daw); daw-audio-sync is REAPER's (adapters/reaper)
- The sync engine, its peer network and Link, one crate
- Rigs below signal-ui: the piano they share is signal-widgets'
- Macromod is a DSP library (features/dsp/macromod), not a rig
- Time UI in the UI crate: dsp-core's time::ui is fts_audio_ui::time
- Dsp-core as the workspace has it
- The dock model and its Dioxus host, one crate
- The plugin GUI glue, one crate
- Editor builds on gui, as fts-plug-ui did through plugin-core's defaults
- The shared editing model moves to features/editing
- The editing model the arrangement and the Editor share, one crate
- The app chrome is its chrome module
- Fts-themer-ui is an app (apps/themer), not a UI library
- Its tests build with link and network
- No second copy of audio-controls
- Remove signal-ui and fts-themes: nothing used them
- Drop the widgets frame replaced
- Drop the canvas EQ graph, transfer curve and MIDI keyboard
- Tone3000 and space: each crate's parts in one folder (before folding)
- One crate
- The account proto's vox is the crate's vox
- One crate
- :note_name: one spelling of a note
- Session-desktop --engine on architect's EngineHost
- Meters follow the track set; the one-engine plan
- Engine and Rig: one engine per process, rigs as folders in it
- Folders within folders
- Engine and Rig build on wasm (the device stays native); rig-host takes midi everywhere
- Keys rig on the one engine
- Sampler rig, drums and ekit on the one engine
- One engine for Signal; the MIDI hub in midi; Session takes hardware MIDI
- Fold legacy/ in; one copy of the app shell, loudness and tempo
- The feature layout: signal, rigs, adapters per host, the editor
- Move the old cell readme to docs/session-cell.md
- Timeline step 1: rename features/editing to timeline
- Timeline step 2: characterisation tests for the arrangement's camera
- Timeline step 3: input — Mods, ModKey, Clicks, AxisLock
- Timeline step 4: time — TimeMap, ConstantTempo, TempoMap
- Timeline step 5: grid — Grid, Feel, GridRegions, lines
- Timeline step 6: snap — Snapper and SnapRule
- Timeline step 7: camera — TimeView, RowView, Camera, Bounds (the Editor)
- Timeline step 8: the arrangement's camera on timeline::Camera
- Timeline step 9: rows — RowLayout, RowModel, stack
- Timeline step 10a: one zoom drag — ZoomDrag down-in, rebase at limits
- Timeline step 10b: ZoomHistory
- Timeline step 10c: contextual zoom in timeline; SmartZoom goes
- Timeline step 11: cursor and selection
- Timeline step 12a: one mouse-map table, no binding changes
- Timeline step 12b: Shift flips snapping in the arrangement
- A Razor tool, as the Editor has
- Ctrl is the razor on notes and edges; no x key
- Timeline step 13a: shared hit testing — part, razor_hit
- Timeline step 13b: the drag lifecycle and trim, shared
- Timeline step 13c: the Editor's roll decides clicks on release
- Timeline step 14: the Editor's undo is the song's Loro doc
- Timeline step 15: one clipboard; the arrangement copies and pastes
- Timeline step 16: scroll and touch; edge scroll; one wheel
- Timeline step 17: the shared actions — TIMELINE_*
- Timeline's optional architect deps (the actions feature)
- One engine for quantize and level, acting on the selection
- Backspace takes the automated drum edit off, then the hand placements
- Edits queued for the window land on a barrier, not a poll
- :join: each child guid asked for in one step, not a scan
- Where each edit's time goes, kept and read back
- A song's history folds into a snapshot; an open decodes it, not replays it
- Reading a song back: one call for every take, not three an item
- Stretch markers render through the phase vocoder, not WSOLA
- :comp: a part's takes ranked for a stretch
- One snap for every gesture: the grid, magnets, and Shift flipping it
- Comp ranking: T and Shift+T step through a stretch's takes, best first
- HITS_DETECT takes a call without settings
- Hit detection: takes found in parallel, plain WAVs read in one pass
- Every action timed under any log level (timing::DIRECTIVES)
- T comps every mic from its own recording of the take, in 90 ms
- A song's history folds once it is left alone, not after 64 steps
- Edit mode shades gaps on the kit's rows only, and only an edit's
- The lane grid thins and fades as it crowds
- A slice's waveform is cut from its recording's, never read again
- A shut folder folds in a sweep, and the window's catch-up is timed
- A shut folder's spans are mixed again only when what they hold changed
- The engine journals what it writes; T reads back only what changed
- :placement: the recording played with its hits moved, from scratch
- A hit dragged is placed again from the recording: moved back, it is as it was
- Rust-dx (API shape and performance) and perfect-functions
- :id: a project, track, item and take each have their own id type
- An undo step is what it changed, not a copy of the song
- Item edits are values applied under one lock; a hit move plans in 0.07 ms
- :Song: a song open in the engine, handed down instead of read from globals
- Just drums-bench: the whole drum edit of a real song, timed end to end
- Where the hits play is an index kept from the journal, not a rescan
- The whole song comped from its best takes as one edit: Ctrl+T, COMP_BEST
- Quantize places the kit again from the recording; no edit leaves a flam
- A drag reads only what it changed: 33 ms a drag, the whole edit 2.03 s
- A quantize reads the song once for all its spans: the drum edit in 1.8 s
- Edit mode shows which take the comp plays where, along the kit row's foot
- A row showing its lanes stacks them tall, its head pinned while you scroll its takes
- Every take its own named colour: on its lane, its comp pieces, the comp strip, and COMP_CHOOSE
- Take lanes: kit-coloured waveforms on a stronger take-coloured body; pin only the top lane
- A comp takes the overheads and rooms from the same take as the close mics
- Comping never leaves a hole or another take under a mic; a clearer lanes button
- The drum-edit check tells a hit by its recording and id
- COMP_CHOOSE and COMP_CYCLE cut their edges before the hits there
- Whether the drum samples play the hits where they play now
- Comp ranking scores how tight a take is, not how much of the chart it plays
- The kit's hits as MIDI, both ways: a Drum Hits track and each Sample lane
- Comp ranking: a few hits prove little about how tight a take is
- Drum samples pull with their close mics: DRUMS_SAMPLES_ALIGN
- Comp strip: pick a take's spans to comp with T / Shift+T
- Comp seams where they are heard least: editing::seam, COMP_SEAMS
- Comp ranking: takes alike to the part first, then the tight ones
- Keep the take chosen for a stretch (Ctrl+Shift+T)
- Items over MCP: ITEMS_SET, ITEM_ADD, ITEMS_DELETE, ITEM_SPLIT
- One main take, left only where it has to be
- Rank takes by hand: Up/Down, mouse buttons 4/5, while recording
- Take markers: stars, a red X and notes; take regions; comp lanes select
- Comp lanes: swipe from a take, rank the lane's take, gaps marked; SESSION_FRAMES
- Session's global app menu over D-Bus; the window's app id is `session`
- The scroll reaches every lane; Edit mode hides the takes marked bad
- Lanes zoomed in keep their fine waveforms; a frame's building is budgeted
- Kept waveforms outlast arriving detail; text measured from a table; X'd takes hidden on open
- A take cut at every hit draws as one item; a frame measures less again
- Zoomed-in waveforms hold still
- An edit never leaves the kit silent: a slice is drawn out whatever it takes
- The lanes a track plays share one read of its detail again
- An edit zone changed, or a point of interest, is kept in the song's file
- A render worker is as realtime as the audio callback on Linux
- Every plugin has a lock of its own; renders read the map
- A render covers a part of the song: the tracks ahead, or the live ones
- The tracks nobody plays live render ahead; the callback renders the rest
- Audio health says how the render ahead is doing; SESSION_AUDIO reads it
- Architect's action-list at fc4db34: stage timings and the quiet span filter
- An edit to a track rendered ahead is heard at once, not the depth later
- Play, a locate, an armed track: the tracks ahead play on without a gap
- A track worked on renders live: its plugin open, played over MIDI, or kept so
- A meter on a track rendered ahead moves with what is heard, block by block
- The render ahead aims as deep as the machine needs; an edit builds one snapshot
- Session's Settings set the render ahead: on or off, and how far
- Audio health says what changed, and warns when a block missed
- Signal's Gig page: the set in order, each song's chart and its practice track
- A worker seat at a priority builds on macOS again
- Signal-desktop's gig player deps (cpal, fts-sample, stretch)
- Gig player: count the frames before borrowing the block
- Gig player: an output that hangs is given up on; time in the track's own rate
- The instrument in the top bar — Guitar or Bass, or back to the menu [skip ci]
- Leaving a rig is Setup → Instruments, not a top-bar switch [skip ci]
- IPad drums: a 4×4 pad grid to play the kit, and its pieces to pick [skip ci]
- Rigs share one perform system, content moves between devices, a new front page [skip ci]
- Port ios-framing to studio: crate paths and names, rustfmt
- The ported crates (signal-rig-shell, -perform-ui, -perform-store, signal-content-*)
- Gig player decodes compressed tracks (ogg/mp3/flac/aac) so the iPad can carry the set
- Signal iPad gig shell: the rig's header, Setlist sidebar and Perform middle carry the gig (chart PDF/Keyflow, transport, Studio/Signal, progress bar); gigs ship inside the app; nice-plug builds for iOS
- IOS scripts: studio paths, bundle gigs into the app (GIG_BUNDLE); gig-ipad.py: one encode per shared track
- Gig shell: header fits the 11-inch top bar (song steps move to the progress strip); the player opens with the first song, not at launch
- Gig shell: FTS_GIG_SONG opens on a song
- Frame sits beside the studio checkout (one level deeper than the old repo)
- Build against studio's Blitz (no frame_stats there yet; the health log reads zeros)
- Gig shell: the chart gets the room (switches off under a gig, Macros brings the rig back); FTS_GIG_VIEW / FTS_GIG_PLAY for screenshots
- Gig shell: a tempo that is more than a number reads as written; rustfmt
- Airlock-ios skill: build Signal for iOS from the studio checkout
- Gig shell (STA-179): the rig's own Setlist view carries the gig (its set seeded as a rig setlist, sections as parts; the chart follows the song up); Session's SongProgressBar; chart controls in the foot bar; per-song zoom/offset per view, kept on disk; Session / Signal
- Gig shell (STA-183): compact switches (1–5, one short row), the default under a gig; a hold on Switches picks Full / Compact / Hidden; chart zoom goes down to 0.4x
- Signal and Session TestFlight builds share one marketing version (0.0.4)
- Compact height is a float
- The foot's chart tools ride as the gig's slots (no fn-pointer compare)
- CoreMIDI init only with the midi dep (signal feature); the iOS-set host build compiles
- Cargo fmt the dev tip (15 rustfmt hunks in 5 files) ([#15](https://github.com/FastTrackStudios/studio/pull/15))

### Docs

- Next steps after the feature layout
- Timeline §5 decisions
- Timeline migration progress, steps 1-6
- Timeline step 10 progress, what stayed

### Build

- Dev-first workflow — light checks into dev, full checks + changelog + tags on dev -> main releases
- *(release)* Release branches cut from dev, so dev never freezes; guard, refresh and back-merge
- *(release)* IOS TestFlight only from a per-app release tag; marketing version = the GitHub Release's (STA-181) ([#7](https://github.com/FastTrackStudios/studio/pull/7))
- *(release)* One release for Signal + Session: single vX.Y.Z tag/version/Release drives both TestFlight uploads (STA-198) ([#11](https://github.com/FastTrackStudios/studio/pull/11))
- *(release)* The guard's bootstrap exemption is 'main has no scripts/release.sh', so PR #1 passes its own guard even though dev exists
- *(release)* Flow v2 — main is finished features, a v* tag is the release (promotion PR, release PR, tag ships sites + both TestFlight uploads + REAPER + Release)
- *(release)* Prepare refuses a merged-but-untagged release even before the first tag exists
- *(checks)* Wasm check and nextest move to an advisory job until STA-184 is green; Build + test stays the required gate
- *(release)* The first release may be main's own version (v0.1.0), changelog only ([#16](https://github.com/FastTrackStudios/studio/pull/16))
- *(signal-ios)* Ship the drum and e-kit rigs and find frame's faces on the runner ([#18](https://github.com/FastTrackStudios/studio/pull/18))

