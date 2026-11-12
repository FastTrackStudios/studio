# Run it all locally

Studio's share page, its extensions, the issuer, the web app and the site,
on one machine, nothing from the network at run time. Change Studio,
Keyflow or Architect and see it in a browser (or on your phone) in
seconds, before anything is merged, tagged or deployed.

```sh
just dev       # everything, rebuilt on save
just demo      # the quickest look: issuer + node + share link, nothing watched
just website   # the public site (Architect's apps/site), hot reload
just phone     # the running node on your tailnet over HTTPS, for a phone
```

The same three recipes mean the same in Architect and Keyflow: standing
there, `just dev` runs this stack with that checkout as the local piece.
`devenv up` (processes in `devenv.nix`) is the engine underneath;
`scripts/local-stack.sh` is each process, `local-stack.toml` its settings.

## What `just dev` starts

| Process | URL | What it is |
| --- | --- | --- |
| `mock-oauth` | http://127.0.0.1:7420 | Architect's stand-in for GitHub, Google and TONE3000 |
| `auth` | http://127.0.0.1:7421 | Architect's auth server: the fixed dev cast (`ada@local.test`, `grace@local.test`, `alan@local.test`; password `development-password`), never a mirror of production |
| `node` | http://127.0.0.1:7422 | Studio Architect's headless node (`studio-architect serve`) built from this tree, its island beside it as the release ships it, signed in to the local issuer as Ada, hosting `acme` with a **copy** of the test vault and a share link of its setlist |
| `web` | http://127.0.0.1:7423/app/ | Architect's web app with Studio Architect's `distribution.json`, its issuer the local one |
| `site` | http://127.0.0.1:7424 | Architect's public site (`apps/site`), Vite with hot reload |
| `desktop` | a window | Studio Architect's app, off unless `just dev desktop=on` |

The share link is fixed, so it is the same on every start and machine:

```
http://127.0.0.1:7422/s/merrybrightstack/setlists/merry-bright--cbu-christmas-2026/
```

The node's log prints it when the node is up; `just dev-status` prints it,
the versions and every URL. The test vault is
`../setlists/merry-and-bright/vault` (`[vault]` in `local-stack.toml`,
`LOCAL_STACK_VAULT` names another); the stack copies it once into its
state and writes the share note there, so nothing done while testing
reaches the source. Without it the node holds Architect's demo seed
(setlist Fall Show, `/s/ksqx75iaaaaaaaae/`).

The issuer, the node's environment, its sign-in and the guard that keeps
everything on loopback are Architect's own `cargo xtask dev`, run from the
Architect checkout the stack resolves, given this stack's ports and state.
Nothing of it is copied here.

## Which pieces are local

Studio is always this tree. Architect and Keyflow are pinned (the tags
`Cargo.lock` names) unless you say otherwise:

```sh
just dev                                   # Studio's tree; Architect, Keyflow pinned
just dev keyflow=local                     # + ../keyflow
just dev architect=local                   # + ../architect
just dev architect=local keyflow=local     # all three
just dev keyflow=/path/to/keyflow          # any checkout
just dev desktop=on watch=off              # the window; no rebuilds
```

`local` means the sibling checkout (`[sources]` in `local-stack.toml`,
tried from this checkout, then from the main checkout of a worktree;
`LOCAL_STACK_ARCHITECT_PATH` / `LOCAL_STACK_KEYFLOW_PATH` name another).
When it is missing the stack says so and keeps the pinned tag.

A local checkout goes under the build as a `[patch]` in a generated cargo
config (`.devenv/state/local-stack/overrides.toml`, passed with
`--config`), never in `Cargo.toml`. Cargo writes the patched sources into
`Cargo.lock` while it builds; the stack puts the committed lock back after
every build (do not edit `Cargo.lock` while a build of the stack runs).

**One Architect.** Studio and Keyflow must build against the same
Architect. `architect=local` patches it for both, so a pinned Keyflow
compiles against your Architect (v0.5.3 does against Architect's `main`
today); if it does not, take `keyflow=local` on a branch that does.
`keyflow=local` with a pinned Architect needs your Keyflow on the
Architect tag Studio pins: the stack checks and stops with what to do
when they differ.

The stack prints, at the top of every process's log, exactly what it
runs:

```
  STUDIO'S LOCAL STACK (dev; watch on; desktop off)
  studio     THE TREE …/studio @ v26.11.3-2-g49eae882-dirty (feat/local-stack)
  architect  LOCAL …/architect @ v0.12.5-1-g5426159d-dirty
  keyflow    pinned v0.5.3 (b71833eddacf), Cargo.lock
  overrides  …/.devenv/state/local-stack/overrides.toml (cargo --config; never committed)
```

## What a save costs

Measured on THEBATTLESHIP on 2026-11-12, with other builds running (load
10 to 70), the node in the `dev` profile. A one-line change, from the save
to the node answering with it:

| Change | Mode | Cycle | What happens |
| --- | --- | ---: | --- |
| `player.js`, `site.css`, `site.js` (Architect's `features/sites/assets`) | `architect=local` | 3.1 s | no build: the debug node reads them from the checkout (`ARCHITECT_SITES_ASSETS_DIR`) and the stack rebuilds the share pages; reload |
| `features/setlists/src/follow.rs` (a Studio extension) | default | 13.8 s | incremental build, relink, restart |
| `features/ui/transport-ui` (the island) | default | 15.6 s | the island's `wasm-release` build and Architect's `build-web`, copied beside the node, restart (no relink) |
| Architect `features/sites/src/url.rs` | `architect=local` | 35.4 s | Architect's crate and everything above it, relink, restart |
| Keyflow `engraver/…/export/follow.rs` | `keyflow=local` | 40.6 s | the engraver and everything above it, relink, restart |

Starting:

| | Cold | Warm |
| --- | ---: | ---: |
| the node (`dev` profile, Architect and Keyflow pinned) | 177 s | a few seconds when nothing changed |
| the island | 40 s | 11 to 15 s |
| the issuer (Architect's xtask, mock-oauth, auth) | about 5 min, once per Architect source | under 10 s |
| the web app | 151 s | |
| the site | 10 s | 10 s |
| `just demo`, to the share page answering | | 19 s |
| `just dev`, node, web and site answering | | 32 s |
| switching to `keyflow=local` / `architect=local` / back to pinned | 39 s / 220 s / 19 s | |

Every Rust change to something linked into the node (Studio's extensions,
Keyflow, Architect) relinks and restarts it: a node cannot yet take a
development copy of a linked extension's share-page renderer
(FastTrackStudios/architect#626). What needs no relink: Architect's share
page assets (with `architect=local`) and the island (a restart, about a
second).

Nothing here needs a release to be seen: the share page, its player, the
island and the charts are all built from your trees.

## On your phone

The share page needs a secure context (it checks the island's files with
`crypto.subtle`), so plain `http://<lan-ip>` will not do. With the stack
running and the phone on the same tailnet:

```sh
just phone        # prints https://<this machine>.<tailnet>.ts.net:10000/s/merrybrightstack/…
just phone off
```

It is `tailscale serve` (HTTPS with the tailnet's certificate, reachable
from the tailnet only), never `tailscale funnel`. Tailscale serves HTTPS on
443, 8443 and 10000; set `[phone] https_port` when 10000 is taken. The
first time, Tailscale may refuse a user who is not its operator: run
`sudo tailscale set --operator=$USER` once. The node needs no idea of its
public name for a share page: its redirects are relative, its CSP is
`'self'`, a link share sets no cookies. Signing in from the phone is not
covered: the issuer and the web app stay on loopback.

## Checking it

```sh
just dev-smoke    # SCREENSHOTS=<dir> keeps the browser's shots at 390 px
```

It starts the stack when it is not running (and stops it after), then
checks that the share link answers, the setlist page lists its songs, the
node serves Studio's island and every file its lock names, and that a
headless Firefox (WebDriver BiDi, `scripts/local-stack-smoke.mjs`) plays a
song and sees the playhead move and the chart follow. The browser runs in
a mount namespace of its own with an empty runtime dir and `/run/pipewire`,
so it can never open an output on this machine's audio graph; without the
namespace the check refuses to run.

It is not a CI job: it needs the private test vault and a node build
(177 s cold on THEBATTLESHIP), which no free runner has.

## Running next to other work

- Ports 7420 to 7424 (`local-stack.toml`, `LOCAL_STACK_PORT_<NAME>` moves
  one) are clear of Architect's own stack (7410 to 7414), so both run at
  once.
- State is `.devenv/state/local-stack` in this checkout: the vault copy,
  the issuer's database, the node's data, the staged node, the overrides.
  `just dev-reset` removes it (stop the stack first).
- Builds go to this checkout's target dir. A local Architect's builds for
  the stack (its xtask, the issuer, the web app) go to
  `target/local-stack/architect/target`; the pinned Architect is a clone
  under `target/studio-architect/`, sharing a local Architect checkout's
  git objects when it has the commit. The stack writes nothing into the
  Architect or Keyflow checkouts, except the site's `node_modules` when
  the site runs from a local Architect (its hot reload needs it there).
- Ctrl-C (or `devenv processes down`) stops everything and puts
  `Cargo.lock` back.
- Nothing plays audio: the node serves files.
