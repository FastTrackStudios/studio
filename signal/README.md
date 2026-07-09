# fts — the FastTrackStudio monorepo

One tree for the whole stack. Every domain that previously lived in its
own repo is a subtree here (history preserved); intra-stack dependencies
are plain path deps, so cross-cutting changes are one commit and version
drift is structurally impossible.

```
signal/                  # live rig: chains, NAM, perform surfaces
daw/                     # engine, audio-io, proto, standalone, backends
session/                 # setlists, songs, charts — the session domain
keyflow/                 # chart/keys analysis + writing
midicore/                # MIDI facade
input_actions/           # actions/keybindings framework
Plugins/FTS-Audiocore/   # shared DSP + gui primitives (audiocore)
FastTrackStudio/         # shared utils
FTS-Plugins/forks/       # fts-plug (nice-plug / nice-plug-dioxus)
neural-amp-modeler-rs/   # NAM core binding (vendored C++ core)
fts-ui/  dock-dioxus/    # UI component libraries
apps/fasttrackstudio/    # THE app: chart writing (keyflow), setlists,
                         # daw integration, Signal + Session usage —
                         # feature-configured, one binary
```

**architect stays external** (framework cadence, consumed like a
crates.io dependency).

Directory names intentionally match the old sibling layout so every
existing relative path dep resolves unchanged — phase 1 is repo
unification with zero build breakage. Phase 2 merges the workspaces
(one lockfile, one shared `target/`, feature-gated cold builds).

Dev shell: `nix develop` (signal/flake.nix is the reference shell until
the root flake lands).
