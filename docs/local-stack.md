# Run it all locally

ONE command brings up the whole local setup — the Studio engine, Studio
(the app, its own server), and Studio Web — rebuilt on every save:

```sh
devenv up        # or: just dev
just up          # the same, as a systemd user unit that survives its terminal
just down        # stop that unit
just dev-status  # what runs, the share link
just dev-sync    # the vault copy, again from its source (`just dev-sync watch` follows it)
just phone       # Studio Web on your tailnet over HTTPS, for a phone
```

## What `devenv up` starts

Three processes (`devenv.nix`, each `scripts/studio-dev.sh run <name>`),
rebuilt on a save:

| Process | Where | What it is |
| --- | --- | --- |
| `engine` | its Action List on http://127.0.0.1:47821/mcp | **The Studio engine**, headless (`studio engine`, from a stable copy of the `studio` binary). Every window attaches to it (`FTS_ENGINE=attach`), so a rebuild of a window never takes the audio down. When it is up it opens `[studio] open` (`studio open`) |
| `studio` | a window; its server on http://127.0.0.1:7412 | **Studio**: Studio Architect (`apps/architect`: Architect with Studio's extensions, Lighting and the DAW view linked in), `dx serve -p studio-architect --platform desktop --renderer native --profile desktop-dev --target <host>`. It is the server too (`STUDIO_SERVE=1`: its node runs in the app's process, holding a copy of the test vault and its share link) |
| `web` | http://127.0.0.1:7413/app/ | **Studio Web**: Architect's web app under `dx serve --platform web` (`cargo xtask dev run web` in the Architect checkout, as Architect's own dev runs it), with Studio Architect's `distribution.json`. Its dev server proxies the node's paths (`/s`, `/vox`, `/org`, `/node`, `/_ext`; WebSockets too) to Studio |
| `studio-desktop-legacy` | a window, **off** | TEMPORARY: the old Studio window (`studio-desktop`, the DAW view), attached to the same engine, opening `[studio] open`, until Studio desktop is merged into Studio Architect (the DAW view becomes the view you enter inside it). `devenv up engine studio studio-desktop-legacy web` adds it |

A share link is the web app's page (Architect #646): the node answers
`/s/<code>/` with a page that loads `/app/start.js` and the app's bundle
from its own origin. Opened on Studio Web's origin, it gets both: the page
from Studio (proxied), the app from dx. That is the same-origin
pair Architect's dev uses (its `apps/web/Dioxus.toml`); the node is told so
with `ARCHITECT_WEB_APP=beside`. The share link (`just dev-status`, the
studio process's log):

```
http://127.0.0.1:7413/s/<share_code>/<setlist page>/
```

There is no issuer: Studio runs signed out, on its local vault, and
a share link needs no account. Signing in to the web app is not covered
(its `config.json` names Architect's dev issuer, which this stack does not
run; `just dev-legacy` does).

**The engine.** The engine and every window of the stack use the XDG
data/config home in `[studio] profile` (default: the stack's own, under
`.devenv/state/studio-dev/profile`): the engine registers there, so the
stack's windows find it and never the engine of a Studio started some
other way (and that one never finds the stack's). The `engine` process
refuses to start when an engine already runs in that profile. Its Action
List is on `[studio] engine_mcp_port` (47821), apart from a normal
Studio's 47811. Stopping the stack (`just down`, or Ctrl-C) stops it.

**Lighting** is a Studio extension (`ignition`), not a process of the
stack: it comes up inside Studio (⌘K "Lighting: show the room")
and runs its own renderer (`ignition --process-view`, built by the stack:
`IGNITION_BIN`). Its settings (`ignition.*`, Settings → Lighting) say what
it runs; `[studio] live_set` is its live set while `ignition.live_set` is
empty.

## This machine's settings: `local-stack.local.toml`

Which vault, setlist and share link, which set Studio opens, and whether
Architect and Keyflow are local go in `local-stack.local.toml` beside
`local-stack.toml` (gitignored): any table or key of `local-stack.toml`,
merged over it. Never edit the committed file for a machine's choice. The
`LOCAL_STACK_*` variables (`LOCAL_STACK_VAULT`, `LOCAL_STACK_SETLIST`,
`LOCAL_STACK_SHARE_CODE`, `LOCAL_STACK_OPEN`, `LOCAL_STACK_LIVE_SET`,
`LOCAL_STACK_PROFILE_DIR`, `LOCAL_STACK_ARCHITECT_PATH`, …) win over both,
but a bare `devenv up` from a unit does not see a shell's variables: put
lasting choices in the file.

```toml
# local-stack.local.toml
[choose]
architect = "local"        # Lighting and STUDIO_SERVE need Architect's main
keyflow = "local"

[sources]
architect = "/run/media/Development/architect"
keyflow = "/run/media/Development/fts/keyflow"

[vault]
source = "/run/media/Development/fts-dev/recital/vault"
setlist = "Setlists/JT's Senior Recital.md"
share_code = "jtsrecitazxmas26"

[studio]
profile = "/run/media/Development/fts-dev/recital/profile"
open = "/run/media/Development/fts-dev/recital/songs/JT's Senior Recital.setlist"
live_set = "/run/media/Development/fts-dev/recital/show/Sr. Recital Showcase Project/Sr. Recital Showcase.als"
```

**The vault** is a COPY of `[vault] source`, made once into the stack's
state (`.devenv/state/studio-dev/vault`), so nothing done while testing
reaches the source. Studio opens it and its node serves it, with
the share link of `[vault] setlist` (`share_code`, fixed: the link is the
same on every start). The source changes while you work: `just dev-sync`
copies it again (what changed comes in, what was deleted goes; what
Architect keeps in the vault, `_architect/` and `.architect/`, stays), and
`just dev-sync watch` keeps following it. `just dev-reset` drops the whole
state (stop the stack first).

**A local Architect or Keyflow** goes under the build as a `[patch]` in a
generated cargo config (`.devenv/state/studio-dev/overrides.toml`, passed
to dx as `--cargo-args=--config=…`), never in `Cargo.toml`. Cargo
writes the patched sources into `Cargo.lock` while it builds; every build
of the stack puts the committed lock back (`studio`: when dx stops). Studio Web runs from
that Architect checkout (or a clone at the pinned tag), its builds in this
checkout's `target/local-stack/architect/target`.

## Surviving the terminal: `just up`

A process started from a terminal or an agent's shell dies with it (an
agent host restart once stopped everything). `just up` runs `devenv up` as
a systemd user unit instead:

```sh
systemd-run --user --unit=studio-dev --collect --same-dir --setenv=PATH="$PATH" "$(command -v devenv)" up
```

It takes the user manager's environment (`WAYLAND_DISPLAY`, `DISPLAY`, the
session bus), which is what the windows need: nothing there is overridden.
`journalctl --user -fu studio-dev` follows it, and each process logs to
`.devenv/run/processes/logs/<name>.std{out,err}.log` (`devenv processes
list` sees only a detached `devenv up -d`, not this one). `just down` stops
it, engine and all.

## Ports and the running Architect stack

Studio's server is on 7412 because Architect's web app proxies the
node's paths there (its `apps/web/Dioxus.toml`, which Studio Web runs
from): this stack cannot run beside Architect's own dev stack (7410–7414).
A `studio-architect-web` package with Studio's extensions and its own
`Dioxus.toml` removes that (a gap in `docs/design/studio-in-architect.md`).
`[studio] web_port` moves Studio Web; `STUDIO_DEV_STATE` puts a second
stack of the same checkout in another state folder.

## On your phone

The share page needs a secure context (it checks the island's files with
`crypto.subtle`), so plain `http://<lan-ip>` will not do. With the stack
running and the phone on the same tailnet:

```sh
just phone        # prints https://<this machine>.<tailnet>.ts.net:10000/s/<code>/…
just phone off
```

It is `tailscale serve` of Studio Web (HTTPS with the tailnet's
certificate, reachable from the tailnet only), never `tailscale funnel`.
Tailscale serves HTTPS on 443, 8443 and 10000; set `[phone] https_port`
when 10000 is taken. The first time, Tailscale may refuse a user who is not
its operator: run `sudo tailscale set --operator=$USER` once.

## The legacy stack

Before Studio served itself, the stack was six processes:
Architect's issuer (`mock-oauth`, `auth`), Studio's headless node
(`studio-architect serve`), the static web app, a **front** (Caddy) that
puts node and web app on one origin, and the site
(`scripts/local-stack.sh`, `devenv --profile legacy-stack up`). It stays
until nothing needs it:

```sh
just dev-legacy                            # all of it, rebuilt on save
just dev-legacy architect=local keyflow=local watch=off
just demo                                  # issuer + node + web + front, last build
just website                               # the public site, hot reload
just dev-smoke                             # its end-to-end check (Firefox)
```

Its share link is on the front, `http://127.0.0.1:7425/s/<code>/…`: the
front sends `/app/*`, the bundle's `/assets/app-*` and `/auth/callback` to
the web app and everything else, WebSockets too, to the node, which runs
with `ARCHITECT_WEB_APP=beside`. The issuer and the node's lane are told
the front is the web app's origin; the bundle's CSP names the local issuer
there. It reads the same settings (`[ports]`, `[sources]`, `[vault]`,
`[choose]`); its state is `.devenv/state/local-stack`.

### What a save costs (legacy stack)


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

### Its phone link

`scripts/local-stack.sh phone` serves its front on the tailnet
(`scripts/local-stack.sh phone off`), as `just phone` serves Studio Web.

### Checking it (legacy stack)

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

### Running next to other work (legacy stack)

- Ports 7420 to 7425 (`local-stack.toml`, `LOCAL_STACK_PORT_<NAME>` moves
  one) are clear of Architect's own stack (7410 to 7414), so both run at
  once.
- State is `.devenv/state/local-stack` in this checkout: the vault copy,
  the issuer's database, the node's data, the staged node, the overrides.
  `scripts/local-stack.sh reset` removes it (stop the stack first);
  `scripts/local-stack.sh sync` copies the vault again from its source.
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
