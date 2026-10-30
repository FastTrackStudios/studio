# Guide media, tours and tests from one scenario

Status: research and proposal, 2026-10-30. Nothing here is built yet.

Studio's guides need pictures that move: a GIF or WebM of the action
happening. They should also become interactive tours inside the app, and
the same steps should run as UI tests. All three have to come from the
real app so they stay right when the UI changes. This note surveys the
libraries that do parts of this, records what Studio already has, and
proposes one mechanism: a **scenario**. A scenario is a small generated
`.session` plus a list of steps. The steps are run headlessly to make
frames, run under nextest to make a test, and run by a highlight overlay
in the window to make a tour.

Sources are linked inline. Code references are paths in this repo at
`66a90197` (branch `feat/no-issue-comp-colours-live-swipe`). Licence and
activity figures come from the GitHub REST API (`gh api repos/<r>`) and
the crates.io API, both read on 2026-10-30. Anything marked
**(guess)** has not been checked.

---

## 1. Libraries

Studio's window is not a webview. It is Dioxus Native on Blitz (Stylo +
Taffy layout, painted by Vello on wgpu). The arrangement, where comping
happens, is a single custom widget that paints its own Vello scene
(`ArrangementWidget`, `apps/session/ui/src/widget.rs:4525`). A lane or a
take is therefore not a DOM node at all. No browser-side library can
drive or inspect it. What carries over from these libraries is their
**data models and conventions**, not their code.

### Comparison

| Tool | What it does | Licence | Maturity (stars, last push) | Can it drive or record Studio? | Idea that carries over |
|---|---|---|---|---|---|
| [driver.js](https://driverjs.com/docs/configuration) | Product tours: highlight an element, show a popover, step through | MIT | 27.0k, 2026-10-29 | No (DOM only) | Step = `element` (selector **or a function returning the element**) + `popover {title, description, side, align}`. Hooks `onHighlighted`, `onNextClick`. `advanceOnClick`. `moveNext()` for programmatic advance. `waitForElement`, `skipMissingElement` |
| [Shepherd.js](https://docs.shepherdjs.dev/guides/usage/) | Product tours | **AGPL-3.0 or commercial** (LICENSE.md) | 13.8k, 2026-10-29 | No | `attachTo {element, on}`, where `element` may be a lazily evaluated function. **`advanceOn {selector, event}`**: "an action on the page which should advance shepherd to the next step". `showOn()` to skip a step. For complex triggers the docs say to listen to your own app event and call `tour.next()` |
| [Intro.js](https://github.com/usablica/intro.js) | Product tours | **AGPL-3.0 or commercial** (license.md) | 23.5k, 2026-09-21 | No | Same step shape. The licence alone rules it out for a docs site |
| [reactour](https://github.com/elrumordelaluz/reactour) | React tours | MIT | 4.1k, 2026-05-19 | No | Same |
| [react-joyride](https://github.com/gilbarbara/react-joyride) | React tours | MIT | 7.9k, 2026-07-09 | No | Same |
| [rrweb](https://github.com/rrweb-io/rrweb) | Records DOM snapshots plus incremental mutations and input, then replays them as DOM | MIT | 20.3k, 2026-10-30 | No. Canvas is off by default. `recordCanvas` + `sampling.canvas` falls back to image snapshots at up to 15 fps ([canvas recipe](https://github.com/rrweb-io/rrweb/blob/master/docs/recipes/canvas.md)) | Record **semantic events, not pixels**, and re-render at replay time. Studio's equivalent is the Action List event log plus the session's undo history |
| [Playwright](https://playwright.dev/docs/videos) video and [trace](https://playwright.dev/docs/trace-viewer) | Browser test runner. `video: 'on' / 'retain-on-failure'`; `show.actions` overlays each action's outline and title on the video. Traces hold before, action and after DOM snapshots for each action, plus a screenshot filmstrip | Apache-2.0 | 98.9k, 2026-10-30 | Only for the web builds (`apps/studio/desktop/e2e` already uses `@playwright/test ~1.59` for the browser keys rig). Not the native window | **The test is the recording**: one script, and video is a run option. Overlay the action being performed. Record before and after state for each step |
| [Puppeteer `page.screencast`](https://pptr.dev/api/puppeteer.page.screencast) | Records a page to WebM (VP9, 30 fps) through ffmpeg. Marked obsolete in favour of `Page.record()` | Apache-2.0 | 95.8k, 2026-10-30 | No | Encode by piping frames to ffmpeg |
| [Remotion](https://www.remotion.dev/docs/the-fundamentals) | Composes video from React: a component reads `useCurrentFrame()`, and a composition is a component plus `width/height/durationInFrames/fps` | **Remotion License**: free for individuals, non-profits and companies of up to 3 people; larger companies need a company licence (LICENSE.md) | 69.1k, 2026-10-30 | Can't render Blitz. Could post-produce frames Studio has already rendered (titles, zooms, captions) | **Video as a pure function of the frame number.** That is the property the headless pipeline below needs: frame *n* depends on *n*, not on wall-clock time. (guess, not checked this pass: it renders in headless Chromium and encodes with ffmpeg) |
| [VHS](https://github.com/charmbracelet/vhs) | Terminal GIFs from a `.tape` script: `Output`, `Set`, `Type`, `Sleep`, `Wait /regex/`, `Hide`/`Show`, `Screenshot`. Outputs `.gif/.mp4/.webm`, a PNG frame directory, or `.txt/.ascii` golden files for tests. Needs `ttyd` and `ffmpeg`. `vhs-action` regenerates GIFs in CI | MIT | 21.2k, 2026-10-01 | Not for a GUI. The **model** is the closest match to what Studio needs | One script yields both media (`Output demo.gif`) and a test (`Output golden.ascii`). `Hide`/`Show` skips setup frames. `Wait /regex/` waits on **state**, not on time |
| [gifski](https://gif.ski/) | High-quality GIF encoder (pngquant palettes, temporal dithering, lossy LZW). Takes PNG frames or video: `gifski --fps 10 --width 320 -o a.gif frame*.png` | **AGPL-3.0-or-later** (crate 1.34.0), with alternative licences on request | 5.7k, 2026-06-17 | Yes, as an encoder for frames Studio renders | Use it as an external CLI in a recipe. AGPL-3.0 can be combined with GPL-3.0 code (each licence's section 13), but calling the CLI avoids the question entirely |
| [`gif` crate](https://crates.io/crates/gif) | Pure-Rust GIF encoder, basic quantisation | MIT OR Apache-2.0 (0.14.2) | 164M downloads | Yes, in process | A dependency-free fallback where gifski isn't installed |
| [FFmpeg](https://ffmpeg.org/legal.html) | Encodes WebM (VP9/AV1) and MP4 (H.264) from a PNG sequence or raw RGBA on stdin | LGPL-2.1+; the GPL parts (notably libx264) make the whole build GPL | n/a | Yes, as an external tool | Pipe raw frames in (`-f rawvideo -pix_fmt rgba -s WxH -r 30 -i -`). No frames written to disk |
| [cua-driver](https://github.com/trycua/cua) | Agent GUI automation: accessibility tree, element tokens, pixel input, background delivery. Linux status is listed as beta in its own MCP notes | MIT (the optional `cua-perception` is AGPL) | 31.4k, 2026-10-30 | It drives the **real** window. Blitz's AccessKit tree is thin for a painted widget (guess), so it falls back to pixels | Good for an acceptance or smoke check of the real desktop. Poor for media: nondeterministic timing, needs a display |
| xdotool on Xvfb (`just drive`) | X11 synthetic input on a nested display | n/a (system tool) | Already in `Justfile` | Real window, llvmpipe rendering | The Justfile's notes record why: Wayland won't let a session synthesise input. Wait for the window, not for seconds. Focus explicitly or keys go nowhere |
| loft-input / niri (`scripts/drum-lab.sh gui`) | Input into an isolated Loft (niri) session | in-house | Already used by the drum lab | Real window on Wayland | The drum lab already times the GUI workflow against the Action List workflow |

**Licence summary for a GPL-3.0-or-later repo.** MIT and Apache-2.0
tools are fine. Shepherd and Intro.js are AGPL or commercial, which a
GPL project could legally use but which buys nothing here. Remotion's
company tier depends on FastTrackStudio's headcount, so it is not a
default. gifski is AGPL: run it as a tool. FFmpeg: run it as a tool, and
any build is acceptable.

### What carries over to a native app

1. **The step data model** (driver.js / Shepherd). A step has a target, text,
   a placement and an advance condition. Shepherd's `advanceOn` and its
   advice to "listen to your own app event and call `next()`" map
   directly onto the Action List's `watch` (section 2.4).
2. **A lazily resolved target** (`element: () => Element`, Shepherd's
   evaluation in `before-show`). In Studio a target is a *query*: a lane
   of a track, the Lanes button of a header. The widget resolves it to a
   rect at the frame the step shows, so a layout change moves the
   highlight with it.
3. **The test is the recording** (Playwright `video:`, VHS `Output`).
   One run, and the outputs are options.
4. **Wait on state, not time** (VHS `Wait /regex/`, Playwright's
   auto-waiting).
5. **A frame is a function of the frame index** (Remotion), for
   determinism.
6. **Semantic events, re-rendered** (rrweb). A scenario stores *actions
   and gestures*, never pixels, so its media re-render on every UI change.

---

## 2. What Studio already has

### 2.1 The headless renderer: `blitz_shot`

`apps/session/ui/src/bin/blitz_shot.rs` renders the app's own tree. Its
module docs say "Nothing here draws the arrangement … the tree is the
app's … The painting is `ArrangementWidget`'s, the one painter". It has
three modes:

- **Picture** (default). A `DioxusDocument` with a `Viewport`, a
  `DataUris` net provider so `data:` images resolve synchronously, 12
  settle rounds of `tick` + 20 ms sleep, then `VelloImageRenderer` →
  `image::RgbaImage` → PNG (`picture`, lines 430–441).
- **Benchmark**. `FTS_BLITZ_FRAMES=<n>` drives `FTS_BLITZ_GESTURE`
  (`pan|down|zoom-x|zoom-y`) by **writing the panel's own view signals**
  (`scroll`, `down`, `zoom`) with no synthetic input (`drive`, line 796).
  `FTS_BLITZ_DUMP=<dir>` writes every frame as `NNNN.png`, which is a
  frame sequence any encoder can take today. `FTS_BLITZ_PROFILE`
  writes a pprof flame graph (feature `profile`).
- **Window**. `FTS_BLITZ_WINDOW=1` mounts `mixer_panel::DawPanels`, the
  component the desktop shell mounts; `=animate` runs the gestures on
  screen.

Session setup from the environment (in `open`): `FTS_BLITZ_OPEN`
(unfold folders), `FTS_BLITZ_LANES` (show every lane through
`session_engine::lanes::set_expanded`), `FTS_BLITZ_MODE=edit|record|mix`,
`FTS_BLITZ_SCENE` (row layout), `FTS_BLITZ_ORGANIZE/CHART/GUIDE`
(prepare), `FTS_BLITZ_STAGE`, `FTS_BLITZ_LEVELS`, `FTS_BLITZ_EXPORT`.
`FTS_BLITZ_COMP=Folder:take:from:to` already performs **a swipe comp
headlessly**: it calls `arrange_edit::Editor::comp`, applies the edits
with `engine::apply_now`, reads the project back, and logs the comp
lane's pieces. That is a comp test in all but name.

Limits that matter here:

- The headless document mounts `panel::NativeTree` (arrangement panel and
  chrome), **not** `DawPanels`. It has no winit bridge, no docked mixer
  and no transport (`HeadlessArrangement`, line 487). Its `Engine`
  closure drops edits: "a picture carries out no edits". Edits only
  happen through the Action List and `apply_now` paths.
- `Justfile` recipes: `studio MODE SIZE SCENE` (windowed, release-fast),
  `studio-demo [--animate]` (calls `studio`), `studio-bench GESTURE SIZE
  FRAMES DUMP` (headless benchmark, release). All open
  `{{GOLDEN_DIR}}/template.rpp`.
- `session_ui::headless::Headless` is a renderer that **never reads
  frames back**. It exists to measure throughput (`headless.rs` docs).
  Capture needs the read-back path (`picture`) or a dedicated
  copy-to-buffer.

### 2.2 Input into the widget

- Every window input reaches the arrangement as Blitz `UiEvent`s through
  `Widget::handle_event` (`widget.rs:4584`).
- The swipe is `ArrangementWidget::swipe_gesture` (`widget.rs:1507`):
  - `PointerDown` (main button, no modifier, not a finger) on a take's
    body, on a lane that is not the comp, on a row showing its lanes,
    becomes a `Swipe`;
  - `PointerMove` extends it;
  - `PointerUp` calls `Swipe::comp_args` (nothing if narrower than
    `swipe::SLOP` = 4 px) and then `run_comp`, which runs the Action
    List action **`TRACK_COMP_FROM_LANE`** off the UI thread;
  - the widget then holds a `Landing` and draws the swiped stretch on the
    comp lane until the row's items change (`row_print`), or for at most
    `LANDING_WAIT` (3 s).
- `vendor/dioxus-test` (MIT/Apache, vendored, Blitz-based) already has
  `DocumentTester::send_ui_event(UiEvent)`. It is "the same path a real
  windowing shell drives", calling `document.handle_ui_event`, and it
  also provides `click_at`, `press_key` and `type_text`. Its FTS
  extension `render_png` rasterises on the CPU with
  `anyrender_vello_cpu`, so it needs no GPU. Pointer and key injection
  into a headless `DioxusDocument` is therefore an existing, working
  path.
- Keys: `keys.rs` maps the FTS keybind profile (styx files under
  `features/adapters/reaper/reaper-input/config/`) to window `Action`s.

### 2.3 The Action List over MCP

- `session_engine::studio_engine::mcp` serves `crate::actions::list()`
  on `http://127.0.0.1:47811/mcp` (`FTS_MCP_PORT`, `FTS_MCP=0`). The
  tools come from `architect_mcp` (architect `v0.9.31`, commit
  `89338f6`, `features/mcp/src/lib.rs`): `actions`, `describe`, `run`,
  `batch` (≤ 64 steps, not atomic) and `watch`.
- `/tmp/act-tools/act.sh` and `scripts/drum-lab.sh` are curl+jq clients
  of it. `studio run <ID>` is the CLI's (`apps/session/desktop/src/cli/mod.rs`).
- **`watch`** is `ActionList::events_since(since, max, wait)`, which
  returns `ActionEvent { seq, kind }` with `kind` one of:
  - `Ran { id, ok }`
  - `Changed { topic, detail }`
  - `Registered { id }`

  The history holds 4096 events (`EVENT_HISTORY`, architect
  `crates/architect/src/action_list.rs:190–213`). `Ran` does **not**
  carry the arguments.
- Engine code publishes `Changed` topics, for example:
  - `comp.rs:523`: `changed("lanes", "a lane was comped onto the comp")`;
  - `engine.rs:1639`: a topic for every applied edit;
  - `drum_lanes.rs`: `"drums.lanes"`;
  - `record.rs`: `"transport"`.
- The window registers its own actions while attached (`SESSION_MODE`,
  `SESSION_VIEW`, `SESSION_ZOOM`, `SESSION_COMP_*`, …;
  `apps/session/ui/src/window.rs`, tested in
  `apps/session/ui/tests/it/window_attach.rs`).
- Building a session needs nothing beyond actions: `SESSION_NEW`,
  `ITEM_ADD` ("a new item on `track` — on fixed `lane` if given — at
  `position` for `length` seconds, playing the recording `file`",
  `item_actions.rs:25`), `TRACK_SHOW_LANES`, `TRACK_RENAME`,
  `SESSION_SAVE`.

### 2.4 Is the Action List enough to detect "the user did the step"?

Partly.

- **Yes for steps that run an action.** A swipe runs
  `TRACK_COMP_FROM_LANE`, so the tour sees `Ran { id:
  "TRACK_COMP_FROM_LANE", ok: true }` and then `Changed { topic: "lanes" }`.
  Keys bound to window actions (`SESSION_COMP_NEXT` for T, and so on) are
  the same.
- **Not for every click.** The Lanes button calls
  `crate::lanes::set_expanded` directly (`widget.rs:3467`). It does not
  run `TRACK_SHOW_LANES`, and `set_expanded` (`engine/src/lanes.rs:209`)
  only bumps `folds::touch()`. Nothing reaches `watch`. Pure view state
  (scroll, zoom, selection) doesn't either.
- **Not for "which take".** `Ran` has no arguments, so "swiped from the
  third take" can't be read from the event. It needs a **state
  predicate** read after the event: the comp areas (`comp::taken_from(areas, comp, lane)`)
  or the comp lane's items.

So a step's completion is an **event trigger plus a state predicate**.
The event is the cheap trigger that says when to re-check, and the
predicate is the truth. Where a UI control bypasses the Action List, the
fix is to route it through its action, not to special-case the tour.
For the Lanes button that means running `TRACK_SHOW_LANES`, which the
swipe already does for comping. That follows the repo's own rule that
"every action Studio does is a named action".

### 2.5 Fixtures

- **Golden fixture**: `features/dynamic-template/fixtures/golden/`
  (`template.rpp`, `template.kf`, `media/*.wav`, `scenes/*.rows` +
  `*.png`, `folder-items/*.fold` + `.png`). It is the default project of
  every `blitz_shot` recipe.
- **A synthetic song in code**: `apps/session/ui/src/test_song.rs`. It is
  Unbreakable-shaped, with 19 drum mics of 33 fixed lanes (EDIT, COMP,
  one take per lane) and `open_as_edited()`. It is UI-only (a `Project`
  value, not an engine session), used by `arrangement/tests.rs`.
- **Drum lab**: `scripts/drum-lab.sh` handles `copy`, `open`, `actions`,
  `gui`, `report`, `trace` and `act`. It copies a real `.session` with
  its media into `/run/media/Development/fts-dev/lab/unbreakable-*`.
  Those are private recordings, not fixtures that can ship.
- **`.session` is a binary format**: "a Loro doc in a Turso file"
  (`apps/session/engine/src/project_file.rs:1`), with `-wal`/`-tshm`
  sidecars. Committing one would break the repo's Assets rule (binaries
  stay out of git) and would be undiffable. A scenario session has to be
  **built from source**.
- Take colours come from the **source file names**:
  `take_colors::TakeColors::of([(file, length), …])` and
  `swatch(index)` (`arrangement/tests.rs`, test
  `a_comp_stretch_is_its_source_lanes_colour`, in the WIP commit). Five
  distinct files are enough for five distinct colours.

### 2.6 The ```gif blocks

`docs/guides/reaper/*.md` (and copies under `docs/daw/guides` and
`docs/signal/guides`) contain blocks like:

````
```gif
comping-take-lanes
`t` cycles which fixed lane plays back, …
```
````

The first line is a media id and the rest is a caption. **I found no
producer or consumer.** No `.gif` or `.webm` file exists outside
`target/`. The guide site renders `docs/guides/session/*.md` through
`ssg` (`apps/session/web/src/guide.rs`, task@main). The markdown
renderer in the locked `editor` checkout handles `![[x.gif]]` and
`![[x.webm]]` embeds as `<img>` and `<video>`
(`editor-state/src/markdown.rs:844–852`). It has no special case for a
`gif` fence, so these blocks currently render as code. Scope of the
search: this repo, plus the task and editor git checkouts in
`~/.cargo/git`. They look like **placeholders for media that were never
made**, which is exactly what this design should fill. The id
(`comping-take-lanes`) can become a scenario id.

### 2.7 The new drum guide

`docs/guides/session/comping-editing-drums.md` is on branch
`docs/no-issue-drum-guide` (worktree `.claude/worktrees/guides-dev`) and
is also `.claude/skills/comping-editing-drums/SKILL.md`. Each numbered
step gives a **Window** way (key or click), an **Action** way (an action
id with arguments) and a **Done when** line. That is already the shape
of a tour step: target, action, completion predicate. For example, step 2
"Open the lanes with the folder shut" has target: the Lanes button on
`Drums`; action: `TRACK_SHOW_LANES {track: "Drums", show: true}`; done
when: "the folder's row shows a lane per take".

### 2.8 CI

The workflows are generated by `xtask/workflows`. Test jobs that need a
GPU install lavapipe (`mesa-vulkan-drivers`, `xtask/workflows/src/steps.rs:109–111`),
so wgpu and Vello can run on display-less runners. `dioxus-test`'s CPU
rasteriser needs nothing at all.

---

## 3. Native options, weighed

| | A. Real window + synthetic input (cua-driver, xdotool on Xvfb, loft-input/niri) | B. Headless, deterministic (scenario → `DioxusDocument` → injected `UiEvent`s → Vello frames) |
|---|---|---|
| Fidelity | Highest: the whole app, its shell, compositor and real timing | The app's own tree and painter (`blitz_shot`'s claim), minus the shell. Today that means `NativeTree` without the mixer or winit bridge (2.1); mounting `DawPanels` headless closes most of the gap |
| Determinism | Low. Frame timing, async read-back and window placement vary. The Justfile's `drive` notes list hour-long traps (focus, lock files, readiness) | High once time is virtual (see risks) |
| CI | Needs Xvfb (X11) or a nested compositor. llvmpipe "some tens of milliseconds a frame" (`Justfile` `drive` notes). Wayland forbids synthetic input from a client | Runs on any runner with lavapipe, or on the CPU rasteriser |
| Output quality | Screen capture: compositor scaling, cursor, damage artefacts | Exact pixels at any size and DPI, cropped to a region. A drawn cursor and highlight, never a stray one |
| Upkeep when the UI changes | Pixel coordinates (`drum-lab.sh` hard-codes `LAB_RULER_Y=222`, `LAB_LANES_LEFT=375`) break on any layout change | Targets resolved semantically by the widget (lane *named* `T3` of row `Drums`), so media follow the layout |
| Speed | Real time | As fast as frames render, and frames can be skipped when not capturing |

**Recommendation: B for media and tests, with A kept as an occasional
acceptance smoke run** (`drum-lab.sh gui`, the user's acceptance checks
in memory) that runs the *same scenario* through real input. B is the
only option that is deterministic, runs in CI without a display, and
survives layout changes. It is also mostly built already: a headless
document of the app's tree (`blitz_shot`), event injection
(`dioxus-test::send_ui_event`), frame dumps (`FTS_BLITZ_DUMP`) and an
engine with an Action List (`open_silent`).

Risks to settle in the first layer, roughly in order:

1. **Wall-clock time inside the widget.** `Landing` uses
   `web_time::Instant` with a 3 s `LANDING_WAIT`, and notices fade, flings
   decay and trails draw in over time. A deterministic capture needs an
   injectable clock, a "frame time" the harness advances by `1/fps`.
   Remotion's model, applied to Studio. **(guess)** the number of
   `Instant::now()` sites in `apps/session/ui` is small enough to route
   through one clock. That is not counted yet.
2. **Asynchronous read-back.** `run_comp` runs off the UI thread through
   `open::traced_blocking`, and the widget redraws when the song is read
   back. The harness has to wait for the comp to land (`Changed
   {topic:"lanes"}` and then `row_print` changing) before advancing.
   This is VHS's `Wait`, not a sleep. **(guess)** whether
   `open::runtime()` is up under `open_silent`, which `run_comp` needs
   or it silently does nothing. `FTS_BLITZ_COMP` side-steps the question
   with `apply_now`.
3. **Waveform peaks** and `data:` images arrive asynchronously
   (`blitz_shot`'s 12 settle rounds). A capture must settle before its
   first frame.
4. **GPU and CPU raster differences.** Vello on lavapipe and on a real
   GPU, and `vello_cpu`, may differ by a few pixels **(guess)**. Media
   come from one pinned path. Tests assert on state, not pixels.

---

## 4. The recommended pipeline

```
 scenario.styx ──► build the scenario session (actions only) ──► target/scenarios/<id>/<id>.session
      │                                                               │
      │ steps                                                         ▼
      ├──► guide-capture (headless):  open_silent ─► DawPanels document ─► per step:
      │        resolve target ─► run action | inject UiEvents (tweened) ─► wait(predicate)
      │        frames ─► ffmpeg (WebM/MP4) + gifski (GIF) ─► <id>.webm / <id>.gif / <id>.png
      │
      ├──► nextest: the same runner, no frames, assert each step's predicate
      │
      └──► in-app tour: overlay highlights the resolved target, shows the text,
               completes on watch(event) + predicate, then the next step
```

### 4.1 Scenario session: built, never committed

A scenario's `setup` is a list of actions run against a fresh engine:

1. open the golden `template.rpp` (it has the kit);
2. generate the media;
3. `ITEM_ADD` each take onto its fixed lane;
4. `SESSION_SAVE {to}`.

The result is cached under `target/scenarios/<id>/` and keyed by a hash
of the scenario file and the generator. Because it is built only from
actions, setup and the steps are the same kind of data, and a model can
build the same playground over MCP.

Media for a scenario:

- **Synthesised, seeded, short.** Five takes of a kit, each a few
  seconds, written by a small deterministic generator (a kick as a pitched
  sine decay, a snare as filtered noise, hats as short noise bursts, with
  per-take timing jitter from a seed). They are tiny and need nothing
  from `studio-assets`. They needn't be stored at all: they are
  regenerated from the seed into `target/`. Size still matters for build
  time. Arithmetic: 8 mics × 5 takes × 4 s at 48 kHz mono 16-bit
  (96 KB/s) ≈ 15 MB. A smaller kit (4 pieces) or 24 kHz halves that
  again.
- **Real excerpts** where a guide must sound right go into
  `studio-assets` with a line in `assets.txt`, as the Assets rule says.

Colours: five distinct file names give five distinct `TakeColors`
indices (2.5). The scenario names files so the swatch order is the one
the guide wants.

### 4.2 Scenario format

Authored in **styx**, beside the guide, and loaded into Facet-derived
Rust types. Styx is the repo's config idiom: the keybind profile and
`.config/tracey`. Facet is architect's serialisation, and Facet-derived
types are what action arguments already are (`SwipeArg` in `comp.rs`).
The same file is read by:

- the capture binary;
- the nextest harness;
- the in-app tour, at runtime;
- **(guess)** the docs site, later, for a web tour on `session-daw-web`.

A step needs three things:

- **`do`**: an action by id plus arguments, **or** a gesture whose
  target is semantic. A gesture is what the user does; the harness turns
  it into `UiEvent`s.
- **`target`**: a semantic query the window resolves to a rect.
- **`done`**: an event to wake on, plus predicates to check.

Predicates are **read-only actions with an expected subset of their
JSON result**, so there is no second query language. For example,
`TRACK_ITEMS` (it exists) or a new read-only `COMP_AREAS` returning
`comp::taken_from` per lane. Each predicate is a `check {id, args, has}`
that passes when the action's result contains `has`.

Targets (resolved by the window; layer 3 adds the API):

- `{lane {track Drums, lane "T3"}}`: a lane's band on a row
  (`scene.row_box` + `lane_stack().band(lane)`, as the swipe test does).
- `{span {track Drums, lane "T3", from 2.0, to 4.0}}`: a stretch of it,
  in seconds.
- `{control {track Drums, button lanes}}`: a header control (the TCP's
  hit regions).
- `{node "#toolbar-grouping"}`: a DOM node of the chrome, by id or
  `data-testid` (`dioxus-test`'s `by_testid`).

### 4.3 Capture

- A `guide_capture` bin in `session-ui`, next to `blitz_shot`, sharing
  its `open` + document setup. Its headless document mounts **`DawPanels`**,
  not `NativeTree`, so the chrome is the shell's.
- Virtual time: frame *n* = `n / fps`. Each frame does one `tick` with
  the clock at that time, then a capture.
- A gesture becomes a **tweened** pointer path: down at `from`, moves
  eased over `duration`, up at `to`. All of it goes through
  `document.handle_ui_event`, the path the shell drives. A drawn cursor
  and a click ripple (recording-only overlay layers) make the hand
  visible, since a headless document has none. This is Playwright's
  `show.actions` idea.
- `Hide`/`Show` (VHS): setup and settling frames aren't kept. `hold
  1.0` keeps the last frame for a beat.
- `crop`: a target query or a rect, so the GIF shows the lanes and not
  5120 × 1440 of window. `scale` sets the output size.
- Encoding:
  - `ffmpeg -f rawvideo -pix_fmt rgba -s WxH -r FPS -i - -c:v libvpx-vp9 -pix_fmt yuv420p out.webm`
    (and MP4 via libx264 for Safari) from frames piped on stdin;
  - `gifski --fps 15 --width 960 -o out.gif` from a PNG directory;
  - the `gif` crate as a fallback when gifski is missing.

  Both tools come from the dev shell (`nix develop` would need
  `ffmpeg` + `gifski` added; neither is on `PATH` here today).
- Poster frame: the last frame as PNG, for docs pages and for a still in
  the guide's markdown.
- Media placement: binaries stay out of git. Recommended: generated
  during the site build (the `Images` release job already builds
  studio-web), with `just guide-media [id]` locally. See Open questions.

### 4.4 Test

A nextest test per scenario runs the same runner with capture off: no
frames encoded, a tick per step plus the waits. It asserts every
`done.check`. It belongs in the `test_packages` core set only once it is
fast **(guess: seconds, since there's no audio and no encode)**. Until
then it is an `#[ignore]` + `just` recipe, which follows the
rust-testing skill's rule for "something a human looks at". A failing
step reports its step id, the predicate's actual JSON and, optionally, the
frame at failure: the Playwright trace idea of before and after state for
each step.

### 4.5 In-app tour

- **Overlay**: the dim-and-cutout layer mounts at the root with `inset:
  0` (Blitz has no `position: fixed`; per the blitz-design skill and
  CLAUDE.md, overlays are absolute against the root). The popover is
  ordinary inline-styled Dioxus, kept in the DOM with `display:none`
  when hidden (`which_key.rs` documents why nodes that come and go
  panic). Targets inside the arrangement are drawn **by the widget**: the
  tour writes the resolved target into a shared cell, as `which_key`'s
  `Shared` does, and the widget paints the ring in its live pass
  (`overlay.rs`, "something live over something recorded").
- **Advance**: the tour holds a `watch` cursor (`events_since(next, …,
  wait)`) on the Action List *in process*. On each event that matches
  `done.on` (`Ran {id}` or `Changed {topic}`), it runs `done.check`
  predicates, read-only actions, so they are cheap and safe. If they
  pass, it goes to the next step. A step may also complete on `Next`
  (text-only steps).
- **Same steps**: in a tour, a gesture's `from/to` is shown, not
  performed. The highlight is the target span and the popover says
  "Swipe across the highlighted stretch". `showOn` (Shepherd) becomes
  `skip_if` checks, for example skipping "open the lanes" when they're
  already open.
- **Safety**: a tour opens the scenario session (built locally on
  demand), never the user's song, unless a step is explicitly marked
  safe on any song.

---

## 5. "Comping Drums", end to end

This is a sketch of the styx and proposes names only. The action ids are
real except `COMP_AREAS`, which is new.

```styx
// docs/guides/session/comping-drums.scenario.styx
id comping-drums
title "Comping drums by swiping"
guide comping-editing-drums        // the guide this illustrates

session {
    from golden                     // features/dynamic-template/fixtures/golden/template.rpp
    media {
        generator drum-kit
        seed 7
        seconds 6
        tempo 120
        pieces (Kick Snare "OH L" "OH R")
        takes (T1 T2 T3 T4 T5)      // 5 files -> 5 TakeColors swatches
    }
    // each take onto its fixed lane of each piece, then a COMP lane from T1
    setup (
        {run DRUMS_GROUP}
        {run SESSION_GROUPING, args {on true}}
        {run TRACK_COPY_TO_LANE, args {track Drums, lane T1, to COMP}}  // (guess) args
    )
}

view {size 1600x900, mode edit, scene drum-mixing}
capture {fps 30, crop {track Drums}, scale 960, outputs (webm mp4 gif png)}

steps (
    {
        id open-lanes
        text "Show every take of the kit: the Lanes button on the Drums folder."
        target {control {track Drums, button lanes}}
        do {run TRACK_SHOW_LANES, args {track Drums, show true}}
        done {
            on {changed lanes}      // after the button is routed through the action (2.4)
            check {id TRACK_ITEMS, args {track Drums}, has {lanes_shown true}}  // (guess) field
        }
        hold 1.0
    }
    {
        id swipe-t3
        text "Drag along take 3 over bar 2. That stretch goes onto COMP, in take 3's colour."
        target {span {track Drums, lane T3, from 2.0, to 4.0}}
        do {swipe {track Drums, lane T3, from 2.0, to 4.0, seconds 0.8}}
        done {
            on {ran TRACK_COMP_FROM_LANE}
            check {id COMP_AREAS, args {track Drums}, has {areas ({lane T3, start 2.0, end 4.0})}}
        }
        hold 0.8
    }
    { id swipe-t5, … lane T5, from 5.0, to 6.0 … }
    { id swipe-t2, … lane T2, from 8.0, to 10.0 … }
)
```

What each output does with it:

- **Media** (`just guide-media comping-drums`):
  1. builds the session (cached);
  2. opens it headless in `DawPanels` at 1600 × 900, Edit mode;
  3. *hidden*: settles peaks and images;
  4. *shown*: step 1 runs the action, and the Drums row grows its lanes,
     each take in its colour;
  5. steps 2–4 each tween a pointer drag along a take lane, and the live
     swipe draws the take's stretch on the comp lane as it goes (the
     WIP `paint_swiped` / `replay_swipe`); on release the harness waits
     for `Ran TRACK_COMP_FROM_LANE`, then for the row to be read back,
     then holds;
  6. encodes, cropped to the Drums rows.

  Output: `comping-drums.{webm,mp4,gif,png}`. The guide gets a fence
  ` ```gif comping-drums ` whose first line is now a real id.
- **Test**: the same run with no capture. It asserts after each swipe
  that `COMP_AREAS` has the area from the right lane over the right
  stretch. A colour check uses the
  `take_stretches`/`swatch` unit test that already exists in
  `arrangement/tests.rs`, so the scenario test needs no pixels.
- **Tour**: "Try it" on the guide page (or a Help menu entry) builds and
  opens the playground. It rings the Lanes button and waits for
  `Changed lanes` + check, then highlights take 3's bar-2 band ("drag
  along here") and waits for `Ran TRACK_COMP_FROM_LANE` + an area from
  T3 overlapping the span. The tour should accept a swipe that is
  roughly right (an overlap tolerance in the check), unlike the test.

---

## 6. What to build first (small layers)

Each layer stands on its own and is useful alone. One concern per PR,
per `ship-stack`.

1. **Frames to media, from what exists.** A `just guide-gif` recipe takes
   `FTS_BLITZ_DUMP` frames from `studio-bench` (or a still sequence),
   then `gifski`/`ffmpeg`. Add `ffmpeg` and `gifski` to the dev shell.
   This proves encoding, sizes and cropping with no new Rust.
2. **A virtual clock in `session-ui`.** One `now()` the widget's timed
   pieces (`Landing`, notices, fling, trail) read, real by default and
   set by a harness. This unblocks deterministic frames.
3. **Target resolution.** `ArrangementPanel::rect_of(&Target) ->
   Option<Rect>` for lanes, spans and header controls, built from what the
   swipe test already computes (`row_box`, `lane_stack().band`), plus
   DOM ids for chrome controls. Unit-tested against `test_song`.
4. **Route bypassing controls through their actions.** The Lanes button
   should run `TRACK_SHOW_LANES` and get a `Changed` topic. Audit the
   other `set_expanded`/`folds::toggle` callers. Doing this also gives
   MCP and keys the same paths.
5. **Scenario types and runner (no capture).** Facet types, a styx
   loader, and a runner that opens a session, runs `do` (actions;
   gestures as `UiEvent`s through `handle_ui_event`), and waits on
   `done` through `events_since`. Add the read-only `COMP_AREAS`. First
   test: the comping scenario on a hand-made session, `#[ignore]` +
   recipe.
6. **The scenario session builder**: the seeded drum-take generator plus
   `setup` actions, cached under `target/scenarios`.
7. **`guide_capture`**: the runner plus a headless `DawPanels` document,
   tweened gestures, a cursor overlay, hide/show/hold, crop, and an
   encoder pipe. Produce `comping-drums.webm/gif`. Wire the ```gif fence
   (or `![[comping-drums.webm]]`, which the markdown renderer already
   embeds) to it.
8. **The in-app tour overlay**: root overlay, widget-painted ring,
   popover, a `watch`-driven advance, and opening the scenario session.
9. Later: the web tour on `session-daw-web`; a CI job (on
   `studio-release`) that regenerates media for a tag; the
   real-window smoke (layer A) running a scenario through `drum-lab.sh
   gui`.

---

## 7. Open questions

- **Where do the media files live?** They are binaries, so not in git:
  - build them in the site's image job;
  - or publish them to `studio-assets` per release;
  - or a CI artefact the site pulls.

  Whatever is chosen, the guide markdown references them by scenario id.
- **What does the ```gif fence become?** Teach `ssg` (task@main) a `gif`
  fence that resolves `<id>` to `<id>.webm` + poster, or rewrite the
  guides to `![[<id>.webm]]`, which the renderer already embeds as
  `<video>`.
- **The headless `DawPanels` mount.** `blitz_shot` mounts `NativeTree`
  headless because `DawPanels` needs the winit bridge. How much of that
  bridge must be stubbed to mount the full shell headless?
- **`open::runtime()` under `open_silent`.** Do window-run actions
  (`run_comp`) actually run in a headless document, or does the runner
  call the action itself and inject only the visible gesture?
- **Fixed lanes from `ITEM_ADD`.** Does `ITEM_ADD {lane: n}` create
  lanes past the current count and name them, or must lanes be made
  first (`TRACK_COPY_TO_LANE`, `DRUMS_EDIT_LANES`)? Lane naming (T1…T5,
  COMP) also decides what targets can say.
- **Watch granularity.** `Ran` has no arguments. Should architect add
  the arguments, or a short result, to `Ran`? That would make many
  predicates unnecessary.
- **A watch cursor from inside the window process.** It's in process, so
  `events_since` is a direct call. Confirm it's cheap enough to poll each
  frame with `wait = 0`.
- **Audio in media.** A comping video wants sound. Rendering audio
  offline for the same frames (the engine has offline render,
  `heard_levels`) and muxing with ffmpeg is plausible **(guess)**. Is it
  wanted for v1?
- **Pixel goldens.** Should scenario captures double as visual
  regression (the golden `scenes/*.png` already exist), or stay
  state-only to avoid raster noise?
- **Tour authoring for agents.** The drum guide is also a skill. Should
  the scenario file be referenced from the skill, so an agent runs the
  same steps over MCP that the tour shows a person?
