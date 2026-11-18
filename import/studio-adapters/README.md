# studio-adapters

What connects FastTrackStudios' DAW, Signal and Studio to other hosts and
formats. These crates came here from
[FastTrackStudios/studio](https://github.com/FastTrackStudios/studio) with
their history (studio#510). Studio, the product, takes them from here by
git, every crate at one revision.

| Path | What it is |
|---|---|
| `crates/studio` | `studio`, the umbrella: the DAW (`daw`) re-exported, with the REAPER adapter (`reaper`, `reaper-ui`), project files and conversion (`file`, `convert`, the format parsers), the `daw` command line (`cli`) and the REAPER integration-test harness (`test-harness`) behind features |
| `crates/daw/actions` | `daw-actions`: actions that drive a live REAPER |
| `features/adapters/reaper/daw-reaper` | The REAPER backend of the DAW API (and `daw-reaper-xtask`, the runner of its REAPER suites) |
| `features/adapters/reaper/*` | Around it: the bridge (`daw-bridge`), the extension runtime, audio sync, the Dioxus-in-REAPER plumbing (`reaper-dioxus`, `reaper-embed`, `reaper-hwnd`, `swell-ui`), input (`reaper-input`, `reaper-input-config`, `reaper-kbd`), `reaper-launcher`, `reaper-config`, `fts-icons`, Signal's REAPER extension and controller (`signal-extension`, `signal-reaper-controller`), the dynamic template's extension, `daw-perf-test` |
| `features/adapters/{interchange,ableton,logic,protools}` | Project formats: DAWproject and AAF, Ableton, Logic, Pro Tools |
| `features/keybinds` | Studio's keybind files and profiles (the styx/REAPER keybind format, the shipped profiles compiled in, the which-key overlay) |

Below these: [daw](https://github.com/FastTrackStudios/daw),
[fts-fx](https://github.com/FastTrackStudios/fts-fx),
[signal](https://github.com/FastTrackStudios/signal), Frame and Architect,
all by git. Nothing below depends on anything here.

Studio keeps the REAPER extensions that are its product's own: Session's
(`session-extension`, which is Session in REAPER) and the DAW UI's
(`daw-reaper-dioxus-ext`, which mounts `daw-ui`).

## Build

```sh
devenv shell          # or direnv: the fleet toolchain (rust 1.99.1), kache
cargo check --workspace --all-targets
cargo test --workspace
just assets           # the Logic / Pro Tools fixtures over 1 MB (private studio-assets)
just reaper daw-test  # daw-reaper's suites in a real (licensed) REAPER
```

## In Studio

Studio pins every crate here at one `rev` in its root `Cargo.toml`. A
change here is a PR here, then a bump of that revision in Studio. To work
on both, check this repo out beside Studio and patch the git source with
paths to every crate Studio builds from it (never committed):

```toml
[patch."https://github.com/FastTrackStudios/studio-adapters"]
studio = { path = "../studio-adapters/crates/studio" }
daw-reaper = { path = "../studio-adapters/features/adapters/reaper/daw-reaper" }
# … every crate of this repo Studio builds
```

## Licence

GPL-3.0-or-later (`LICENSE`).
