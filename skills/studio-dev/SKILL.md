---
name: studio-dev
description: "Use when running, testing or changing FastTrackStudio's Studio app (FastTrackStudios/studio apps/studio) or an extension it hosts (Keyflow's chart, Ignition's lighting, the arrangement, mixer, lyrics, rhythm): build and start it, open a set, call its actions over MCP or the control socket, run Luau against it, override an extension live with a checkout, co-develop with daw, Keyflow, Ignition or Architect. Never drive the desktop to test it."
version: 0.1.0
author: Studio (FastTrackStudios/studio skills/studio-dev)
license: GPL-3.0-or-later
metadata:
  hermes:
    tags: [studio, extensions, mcp, luau, keyflow, ignition, rhythm, dioxus, blitz, dev-loop]
    related_skills: [architect, kache, rust-build-efficiency, release-workflow]
---

# Developing against Studio

Studio is one Dioxus app (`apps/studio` in FastTrackStudios/studio, app and
binary `Studio`). It runs on Architect's application framework and
app-shell. Its features are **extensions, compiled in**. Any one of them can
be **replaced live by a development copy built from your checkout**, so
work on Keyflow, Ignition or rhythm never needs a Studio rebuild.

The full guide, which this skill is the short form of, is
`apps/studio/docs/development.md` in the studio repo. Read it before your
first run.

## Hard rules

- **Never use the desktop.** No pointer, keyboard, scrolling, cua-driver or
  ydotool on the person's screen. They are working on it. Drive Studio only
  through its actions (below). Screenshots (`spectacle -b -n -f -o …`) are fine.
- **Never commit** `apps/studio/.cargo/config.toml` (local path patches) or
  the `Cargo.lock` those patches produce.
- **Build in the dev shell** (`nix develop` in the studio repo). kache is
  the rustc wrapper, so a revision anyone on the machine has built before
  comes from the cache. Keep one profile (`desktop-dev`, what dx uses) and
  never set `RUSTFLAGS`.

## Start it

```bash
cd <studio checkout>
nix develop -c cargo build -p daw-extension --profile desktop-dev   # the engine
cd apps/studio
nix develop -c dx build --platform linux --renderer native --features native --no-default-features
../../target/dx/Studio/debug/linux/app/Studio &                          # the app
```

- Studio starts the engine (`daw-extension --engine`) when a session opens.
  The engine outlives the window and logs to `~/.cache/fts/studio-engine.log`.
- **Ignition's room and cue list** currently need four variables set when
  Studio starts: `IGNITION_BIN`, `IGNITION_ROOT`, `IGNITION_LIVE_SET` and
  `IGNITION_CUES` (Ignition#64 removes them). Without them the Lighting
  window is empty and everything else works.
- If one is already running, use it. Check `pgrep -a Studio` before
  starting another, and don't kill a Studio or an engine you didn't start.

## Talk to it

There are **two MCP servers**, each with its own Action List.

| What | Where it is | What's on it |
|---|---|---|
| **The app** | `~/.config/fts/studio-mcp.json` (url, bearer token, fresh each launch; never print it) and the control socket `$XDG_RUNTIME_DIR/fts/studio.sock` | `STUDIO_*` (`STUDIO_SESSION_OPEN`, `STUDIO_HOME`, `STUDIO_ENGINE`, `STUDIO_SESSION_VIEW`, …), `EXT_*` (`EXT_LIST`, `EXT_LOGS`, `EXT_VIEWS`), `STUDIO_EXT_DEV*`, `SCRIPT_RUN` (Luau over every action), and every linked extension's actions (`KEYFLOW_*`, `IGNITION_*`, …) |
| **The engine** | `http://127.0.0.1:47811/mcp`, discovery `~/.config/fts/session-mcp.json`. The file is named `session` for historical reasons; it is Studio's engine, not the `.session` file format | The song itself: `SESSION_TRACKS`, `TRACK_SET`, `SESSION_VIEW`, drum editing (`HITS_DETECT`, `HITS_LIST`, `HITS_PLACE`, `HITS_MOVE`, `ZONES_FIND`, `DRUMS_EDIT_LANES`, `QUANTIZE`, `HITS_AUDIT`, …) |

From a shell, use Architect's client on the control socket:
`architect-actions apps`, `architect-actions --app studio list --filter EXT`,
`architect-actions --app studio run STUDIO_ENGINE`. Build it once with
`cargo install --locked --root target/tools --git https://github.com/FastTrackStudios/architect architect-control --features cli --bin architect-actions`.
Over MCP, send `run` with `{"id": "<ACTION>", "args": {…}}`.

**Open a set:** `STUDIO_SESSION_OPEN {"set": "Setlists/<name>.setlist", "song": "<title or index>", "play": false}`.
`location` (default `studio-disk`) and `space` (default: Studio's current
org) are optional. `view` (`arrangement`, `chart`, `mixer`, …) is optional too.
Then poll `STUDIO_ENGINE` until its status is `Ready(<pid>)`.

## Change an extension without rebuilding Studio

Run these on the app's list, in any build, release included:

```text
STUDIO_EXT_DEV      {"folder": "<your checkout>/<extension crate>"}
EXT_LOGS            {"id": "<extension id>"}        # build output and errors
STUDIO_EXT_DEV_LIST
STUDIO_EXT_DEV_END  {"folder": "<the same folder>"} # back to the linked copy
```

| Extension | Crate folder in its repo | Id |
|---|---|---|
| Keyflow (the chart) | `features/extension` | `keyflow` |
| Ignition (Lighting) | `features/lighting/pane` | `ignition` |

**What happens:**
- Studio builds the crate's process program with `cargo build --bin <[entry]>`.
- It swaps that in for the linked copy once the build is good.
- It rebuilds and swaps on every save to any local crate the extension builds from.
- A failed build keeps the last good copy running.

The override survives restarts until `_END`.

**Limits today:**
- Only extensions that ship a process copy (`[entry]` + `[source]` and a
  `main.rs` serving the same `linked::Extension`) can be overridden.
  Keyflow and Ignition can. The arrangement, mixer, lyrics and navigator
  can't yet (studio#671): change those in the studio repo and rebuild Studio.
- A process copy's **Dioxus** views show a placeholder until app-shell#35
  lands. GPU process views, like Ignition's room, draw either way.

When it works: merge in the extension's repo, move Studio's `rev` pin to
the merged commit, and end the override.

## Co-develop with daw, Keyflow, Ignition or Architect

To change one of those repos *and* Studio's use of it, put `[patch]` tables
with paths in `apps/studio/.cargo/config.toml`. That file is local; never
commit it.
- Patch **every** crate Studio builds from that repo, or two copies of a
  type end up in the graph.
- Architect needs both its `github.com` and `www.github.com` spellings.
- The daw table is in the studio repo's AGENTS.md.

Afterwards, land the other repo first, bump Studio's `rev` lines, check
that `cargo check -p studio-app --no-default-features --features native`
passes without the config file, and commit the new lock.

## Where things are

- **The split:**
  - **daw** owns the edit surface and applies edits (items, takes,
    stretch markers, groups) as one undo step.
  - **Studio** draws that surface (Arrangement) and exposes it as actions
    (MCP, Luau).
  - Extensions like **rhythm** compute plans, such as what slips and what
    stretches.
  - **`.session`** is the file and its sync, not a program.
- **Files:**
  - Songs and setlists are plain files in the Studio folder
    (`apps/studio/docs/storage.md`); Drive syncs them when it is installed.
  - The open work is under FastTrackStudios/studio#680.
