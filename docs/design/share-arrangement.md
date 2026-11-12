# The arrangement view on a share link (draft)

A setlist's share link, on a phone, gets a fourth tab: **Arrangement**. It
shows Session's real arrangement view (the one the web demo runs: timeline,
tracks, sections, zoom, scroll, selection) for the song playing, **locked**,
following the share page's player and seeking it. It runs on every Safari:
no JS Promise Integration.

This note records what exists, what is missing, and the plan. Nothing here
is built yet.

## What exists

- **The view, on the web.** `apps/session/ui/src/web_host.rs`: the panels in
  dioxus-web, each widget (`crate::widget::ArrangementWidget`) in a
  `<canvas>` drawn with vello_hybrid's WebGL2 renderer (iOS Safari 15+ has
  WebGL2). The demo bundle is `apps/session/daw-web` (about 7 MB of wasm).
- **A silent source.** `apps/session/engine/src/web_engine.rs`
  `WebSource::Bundled { name, rpp_url, chart_url }`: the song's `.RPP` and
  `.kf` read into memory (`Memory`), opened with `Media::Deferred`: no
  proxies, no stems, no streams.
- **Locked.** `apps/session/ui/src/options.rs` `LOCKING`: drags scroll and
  move nothing (`arrange_edit.rs`, `widget.rs`, `mixer_panel.rs`), mirrored
  from the shell's lock (`shell.rs`) and toggled by the toolbar's Lock item
  (`toolbar.rs`).
- **The data, on a share.** Studio declares no `[[share_layers]]`, so every
  file a song note links is a page-layer file: a song note links its
  `.RPP` and its `.kf` (its Downloads), so every link, `reference`
  included, serves both. Nothing needs widening for the bundled source.
  What a share does **not** have: a prepared `.session`, proxies, stems
  and peaks (`WebSource::Shared` streams those from a Task link). So the
  view shows what the `.RPP` and the chart give: tracks, items, sections,
  tempo and markers, no waveforms.

## What is missing

Studio:

1. **Bytes in, not URLs.** In the share's sandbox frame `connect-src` is
   `'none'`: a `WebSource::Given { name, rpp: Arc<[u8]>, chart: Option<Arc<[u8]>> }`
   beside `Bundled`, the same `Memory` path without `fetch_bytes`.
2. **An outside clock.** The view follows `crate::engine::Transport`, driven
   by the engine's audio. On a share the page's player is the clock: a
   driver that sets the transport's position and playing state from the
   frame's `architectPage.subscribe` props (`time`, `playing`, `rate`) each
   frame, and sends the view's seeks out as `PLAYHEAD_SEEK` (`architectPage.ask`).
   No audio of its own.
3. **Locked, with no way to unlock.** `LOCKING` set on and the Lock item
   (and every editing toolbar) left out of this entry.
4. **Its own web bundle.** An entry (`mount("arrangement")`) built like
   `just island-web` (wasm-bindgen `--target web`, snippets flattened,
   `wasm-opt` holding the module to Safari 15's features), separate from the
   progress/transport bundle, which stays small.

Architect:

5. **An island view a tab can open.** Today an island has two slots
   (progress, transport). A shell tab targeting an island's view
   (`target = { view = "studio.arrangement/arrangement" }`) opens a
   full-height web view frame in the tab area (#608's web view, #609's
   shell).
6. **The song's files, handed in.** The node gives, per song page, the
   files an island asks for by note property (`session:`, `chart:`), under
   the link's scope and layer exactly as a note's art is
   (`architect_sites::art`, `Entry::art`); the page fetches them from the
   link and hands the bytes to the frame.

## Order

Architect 5 and 6, then Studio 1 to 4, then the setlist's
`[[favourites]]` gains the Arrangement tab. The progress bar and the
transport already run this way (architect#608, studio#461), so the route
is proven on every browser; the arrangement adds a bigger bundle and an
outside clock.
