# Deploying Studio on the web

Studio's web front end is one image, `ghcr.io/fasttrackstudios/studio-web`,
and everything to self-host it lives here:

```
deploy/
├── README.md            <- you are here
├── docker-compose.yml   <- studio-web next to a Task of your own, one origin
└── chart/
    └── studio/              Helm 3 chart (generic defaults)
        └── values-prod.yaml EXAMPLE: studio.fasttrackstudio.app on starcommand
```

(`chart/session` and `signal/` are the old Session and Signal sites; they go
when studio.fasttrackstudio.app replaces them, #224.)

## What the image is

Static files and a static file server, nothing else:

| path | what |
|---|---|
| `/` | the front door: Signal or Session |
| `/signal/` | Signal, the live rig (studio-desktop's web build) |
| `/session/` | Session, the DAW (session-daw-web) |
| `/config.json` | where the apps find Task and fts-auth, written at start |
| `/version.json` | the commit and release it was built from |

There is no server in it. Task and fts-auth do every connection: devices,
peer-to-peer, sync and storage. The only runtime input is two environment
variables, which the container writes to `/config.json` when it starts:

| var | meaning | unset or empty |
|---|---|---|
| `TASK_URL` | Task's base URL, e.g. `https://task.fasttrackstudio.app` | the page's own origin |
| `AUTH_URL` | fts-auth's base URL (the OIDC issuer), e.g. `https://auth.fasttrackstudio.app` | the page's own origin |

It listens on **8080**, runs as uid 65534, and writes only
`/srv/config/config.json` (with a read-only root filesystem, mount a tmpfs
or emptyDir at `/srv/config`). Every response carries `Cross-Origin-Opener-Policy:
same-origin` and `Cross-Origin-Embedder-Policy: require-corp`: the browser
keys rig needs a cross-origin-isolated page for its shared memory. A
resource the apps load from another origin must therefore be CORS- or
CORP-enabled (Task's API is, through fetch and WebSockets).

## Two setups, one image

**Task elsewhere** (studio.fasttrackstudio.app): set `TASK_URL` and
`AUTH_URL` to it. The apps dial those.

**Same origin** (a self-host behind one host): leave both unset, and route
Task's API prefixes to task-server on the same host as studio-web, as
task-web expects (task's `deploy/README.md`, "same origin"):

| prefix | goes to |
|---|---|
| `/vox`, `/org`, `/server`, `/blobs`, `/.well-known` | task-server |
| everything else | studio-web |

## Quickstart: docker compose

```sh
docker compose -f deploy/docker-compose.yml up -d
# Studio: http://localhost:8090   Task's own web app: http://localhost:8091
```

That runs studio-web, task-server (state in the `task-data` volume),
task-web and an nginx `edge` doing the same-origin split for both. A fresh
Task hosts no orgs: create one with the `task` CLI (`task org create
<slug> --home`, pointed at http://localhost:8090), as task's README says.

## Quickstart: Helm

```sh
helm install studio ./deploy/chart/studio \
  --namespace studio --create-namespace \
  --set ingress.host=studio.example.com \
  --set taskUrl=https://task.example.com --set authUrl=https://auth.example.com
```

or start from `chart/studio/values-prod.yaml` (our infrastructure; the
chart's defaults are generic). Values:

| value | default | |
|---|---|---|
| `taskUrl` / `authUrl` | `""` | the container's `TASK_URL` / `AUTH_URL`; empty = same origin |
| `image.registry` / `image.repository` | `ghcr.io/fasttrackstudios` / `studio-web` | |
| `image.tag` | `""` | empty = the chart's `appVersion` |
| `ingress.host` / `ingress.className` / `ingress.annotations` / `ingress.tls` | | one host, path `/` |
| `service.port` | `80` | |

A release named `studio` gives a Service named `studio`: on starcommand,
`studio.studio.svc:80`, where the cluster's Caddy routes
studio.fasttrackstudio.app. There the Ingress is only the record
external-dns publishes, which is why it has no class by default.

## Releases

The chart's `version` and `appVersion` are the repo's one version: the
release commit (`just release-prepare`, scripts/release.sh) rewrites both,
so the chart at tag `vX.Y.Z` deploys `studio-web:X.Y.Z`. That image is
pushed BEFORE the tag exists:

```
release PR squash-merges -> `chore(release): vX.Y.Z` on main
  -> Images (.github/workflows/images.yml): studio-web:X.Y.Z, sha-<short>, latest
just release-tag -> refuses until studio-web:X.Y.Z is on GHCR, public and
                    labelled with the release commit (scripts/ghcr-images.sh)
  -> vX.Y.Z -> Argo CD (`targetRevision: "*"`) deploys the newest tag
```

Roll back by deploying the previous tag's chart, or by pinning
`image.tag` to the previous release: nothing in the image holds data.

## Building the image yourself

Pure Nix, no Dockerfile (`nix/modules/packages/studio-web.nix`). The
image is a `dockerTools.streamLayeredImage`, an executable that streams a
docker-archive to stdout; it builds on Linux:

```sh
$(nix build --no-link --print-out-paths .#studio-web-image) | docker load
docker run --rm -p 8080:8080 -e TASK_URL=https://task.fasttrackstudio.app studio-web:latest

# or push it somewhere of your own, without a daemon:
$(nix build --no-link --print-out-paths .#studio-web-image) \
  | nix run nixpkgs#skopeo -- --insecure-policy copy \
      docker-archive:/dev/stdin docker://registry.example.com/studio-web:dev
```

The files alone are `nix build .#studio-web` (any platform), and
`just studio-web-preview` serves them on http://localhost:8090 the way the
image does (`TASK_URL=… just studio-web-preview` for a configured one).
