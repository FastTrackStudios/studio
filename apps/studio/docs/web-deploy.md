# The web build at fasttrackstudio.app

Studio's web app is served at the root of `fasttrackstudio.app` from the
VPS (`architect-vps`, FastTrackStudios/infra). A signed-out visitor gets
the landing page (studio#706, `src/landing.rs`) at any path; a signed-in
one gets the app (`src/door.rs`).

## The app's paths

The app lives under real paths, each its router's (`Route` in
`src/main.rs`). The front serves `index.html` for every one of them (the
"anything else" row below), so none needs its own rule.

| Path | Signed in | Signed out |
|---|---|---|
| `/` | replaced by `/home` (in place: Back does not loop) | the landing page |
| `/landing` | the landing page, its one way in **Open Studio** (to `/home`) | the landing page |
| `/home` (Home), `/records`, `/setlists`, `/signal`, `/orgs`, `/people`, `/org`, `/library`, `/set/<id>`, `/set/<id>/chart`, `/now/chart`, `/session/<view>`, … | that page | the landing page |
| `/join/<room>` | the room's page | the landing page, offering that room |

Going in from the landing page (Open in browser, Sign in) lands on
`/home` from `/` or `/landing`, and on the link itself from any other
path: a room's link, or any deep link, survives signing in.

## What ships, and from where

| Piece | Where | What |
|---|---|---|
| The bundle | this repo, `scripts/web/bundle.sh` (`just web-bundle`) | `dx build --platform web --release --features web` with no base path, packed as `studio-web-X.Y.Z.tar.gz` (index.html at its root, hashed files under `assets/`, `version.json`) |
| The release | `.github/workflows/web.yml` (`xtask/workflows/src/web.rs`) | on a `studio-v*` tag: built on `thebattleship-release`, attached to the one GitHub Release and signed into its `SHA256SUMS` |
| The deploy | FastTrackStudios/infra | a `studio-web` package naming the asset by tag and hash (as `packages/architect-web`); Caddy on the VPS serves it |
| The node | the Drive node on THEBATTLESHIP (`~/.flake`, `hosts/THEBATTLESHIP/drive.nix`) | it serves Studio under `/studio`, so it answers Studio's calls under `/studio/api` |

A release changes nothing live. The VPS takes a new bundle when infra's
`studio-web` source is moved to the tag (`update.sh <tag>`, a PR) and the
owner deploys it. Until that step is automated, "a release deploys the web
build" is that PR plus the owner's deploy.

## How the root reaches the node

The app always calls its node under `/studio/api` (`door::node_mount`):
under `/studio` on the node itself, and from the root of
fasttrackstudio.app. Studio's own pages (`/orgs`, `/shared/…`) share
names with the node's routes at its root, so the node can't sit at the
root of the same name. The front on the VPS routes:

| Path | Goes to |
|---|---|
| `/studio/api/*` (HTTP and the `/studio/api/sync` WebSocket) | the Drive node: over the tailnet to THEBATTLESHIP's `tailscale serve` on 9444, as `drive.fasttrackstudio.app` already is |
| `/studio/auth/callback` | `index.html` (the issuer coming back; the app finishes signing in, then returns to where it began) |
| any other `/studio/<path>` | redirect to `/<path>` (Drive's "Try again" goes to `/studio/`; the calendar's links are `/studio/set/<id>`, `sets::BASE`) |
| `/assets/*` | the bundle, immutable |
| a file in the bundle | that file |
| anything else | `index.html` (the app routes itself) |

The auth side is already in place (infra#45): the `drive` OIDC client lists
`https://fasttrackstudio.app/studio/auth/callback`, and
`https://fasttrackstudio.app` is in `AUTH_CORS_ORIGINS`.

## The cutover (the owner's call)

Today `fasttrackstudio.app` and `www` are Cloudflare-proxied to the home
cluster's ingress, where the `fts-www` chart (FastTrackStudios/fasttrackstudio.app)
serves the old landing page. The cutover, in order:

1. **The first release with the bundle:** `just release`. Check that
   `studio-web-X.Y.Z.tar.gz` is on the Release and in `SHA256SUMS`.
2. **infra PR** (nothing live yet):
   - `packages/studio-web/{package.nix,source.json,update.sh}`, as
     `packages/architect-web`.
   - **The ingress can't express this site yet**
     (`modules/service/_module.nix`): `fts.ingress.static` has no SPA
     fallback (a missing file is `404.html` with 404), `rewrites` match
     exact paths only, there are no redirects, and `fts.ingress.proxies`
     takes a whole domain and refuses one a static site has. Extend it
     first: an opt-in `fallback = "/index.html"` on a static site,
     redirects (prefix-preserving), a path-scoped proxy to an upstream
     off the host (the drive proxy's MagicDNS resolver, streaming, no size
     limit or timeout, X-Forwarded-* replaced), and a redirect-only host
     (for `www`).
   - `modules/services/studio-web.nix`: the static site for
     `fasttrackstudio.app` from the bundle, with the routes above and the
     `/studio/api/*` proxy to `services.drive.upstream:9444`; `www`
     redirecting to the apex. Set on `architect-vps` with
     `domain = "fasttrackstudio.app"`.
   - `just check` (VM test) green, with the VM test covering the
     fallback, the redirects and the path proxy.
3. **Deploy to the VPS** (`just plan`, then `just deploy`, on the owner's
   word). Caddy can't get the apex's certificate yet: the name still
   points at the cluster, so the HTTP challenge fails. Nothing else
   changes.
4. **DNS** (Cloudflare, the zone `fasttrackstudio.app`): point `@` and
   `www` at the VPS (A `179.66.141.121`, AAAA `2a02:4780:95:16f1::1`).
   Keep them DNS-only (grey cloud), as `drive` and `architect` are, so
   Caddy's ACME challenge reaches the VPS. Then `just cert-now
   fasttrackstudio.app`.
5. **Check:** signed out, `/` shows the landing page. Signing in comes
   back to `/home` with the app. `/studio/api/health` answers 200. A set syncs live
   in two browsers.
6. **Retire the old site:** take `fasttrackstudio.app`/`www` off the
   cluster's `fts-www` Argo Application. Archive the repo once #706's
   landing page ships.

**Rollback:** set `@` and `www` back to the cluster's records. The old
chart keeps serving until step 6.
