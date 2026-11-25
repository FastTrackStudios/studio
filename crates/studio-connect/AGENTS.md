# studio-connect: the UI

Everything Studio draws, and the client it draws through. It **talks to an
engine and never links one**: the engine runs in its own process (today
`daw-extension --engine`, which the app starts), on this machine or another,
and a session opens with no engine running at all (its state is the
`.session` document, `studio-core/session`).

| Folder | What it holds |
|---|---|
| `client/` | `engine-client` (the engine, by address), `fts-account` (sign-in, orgs) |
| `theme/` | `daw-theme`, `themer`, the theme packs |
| `kit/` | the view kit: `daw-ui`, `timeline`, `audio-controls`, `dock`, `daw-view-kit`, `studio-panels`, `studio-ui`, `transport-ui`, `studio-shell` (package `app-shell`) |
| `views/` | arrangement, mixer, MIDI and the expression editor, chart (Keyflow's UI and editor), lyrics, lighting (Ignition's live UI and visualizer), rigs, navigator |
| `plugin-ui/` | `plugin-ui`, `fts-audio-ui`, `preset-browser`, `plugin-faces`, `frame-view`, `frame-editor`: the plugin bundles' GUIs |
| `features/` | library, keybinds, patch list, studio-views |
| `mcp/` | `studio-mcp`, the app's MCP for an agent (`scripts/studio-mcp`) |

## Rules

- **Depend on core and connect only.** No engine crate, ever: what a view
  needs from the engine comes over the protocol (`studio-core/proto`), or
  the type moves down to studio-core. `just boundaries` checks it, and the
  allow-list at the end of `boundaries.toml` holds today's edges, which the
  untangling removes.
- **`plugin-ui/` is also the plugin bundles' GUI** (the one allowed edge
  from `studio-engine/plugins/*`). Keep DSP out of it: algorithms are the
  engine's (`studio-engine/dsp`).
- **Blitz, one pipeline**: the views render through `dioxus-native` →
  Blitz (Vello + wgpu), in the app, in a plugin editor and headless. Load
  the `blitz-design` skill before building or changing a view.
  - Inline styles only, or a static sheet through `document::Style`: Blitz
    loads no external stylesheet. Components must lay out without
    Tailwind; classes are additive.
  - No `position: fixed`/`sticky`, no `overflow: auto` (use
    `overflow-y: scroll`), no `text-overflow`, no CSS `fill`/`stroke` on SVG
    (use attributes).
  - Root `App` components take no props (context through
    `use_context_provider`), so one component serves the app and a plugin.
- **The web build** (`just check-web`): never turn on `keyflow-ui`'s `web`
  feature from a wasm consumer. It pulls dioxus-desktop and openssl, which
  do not build for wasm32.
- **Plugin faces are generated** (`just faces`, into
  `plugin-ui/plugin-faces/generated`), never committed.

## Build and test

```bash
just studio          # the desktop app, rebuilt as you edit
just web             # the web app
just test connect    # every connect crate's tests
just editor serve    # the expression editor alone (just/editor.just)
```

The repos this came from: `docs/imported/{daw-view-kit,arrangement-view,mixer-view,midi-editor,plugins,keyflow,Ignition,frame}/`.
