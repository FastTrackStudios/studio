#!/usr/bin/env bash
# Build Studio's web app for the root of fasttrackstudio.app and pack it as
# the release's web bundle: one static site, no server of its own.
#
#   scripts/web/bundle.sh     # -> target/web/studio-web-<version>.tar.gz
#
# The archive holds the dx output as it is (index.html at its root, the
# hashed files under assets/), plus version.json. The front that serves it
# (FastTrackStudios/infra, the VPS's Caddy) serves those files, gives any
# other page path index.html (the app routes itself), and hands
# /studio/api/* to the Drive node: the app reaches its node there wherever
# its pages are (apps/studio/src/door.rs).
#
# Called by the `web` workflow's build job on a release tag
# (xtask/workflows/src/web.rs); a person can run it by hand.
#
# Env (all optional):
#   VERSION     X.Y.Z (default: `release.sh version`), exported to the build
#               as FTS_RELEASE_VERSION and written into version.json
#   OUT_DIR     default target/web
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# In the dev shell: the pinned toolchain, dx and wasm-bindgen.
if [ -z "${WEB_IN_SHELL:-}" ]; then
    exec devenv shell -- env WEB_IN_SHELL=1 bash "$0" "$@"
fi

VERSION="${VERSION:-$(bash scripts/release.sh version)}"
export FTS_RELEASE_VERSION="$VERSION"
OUT_DIR="${OUT_DIR:-$ROOT/target/web}"
target="${CARGO_TARGET_DIR:-$ROOT/target}"
# dx names its output after the binary (`Studio`).
public="$target/dx/Studio/release/web/public"

echo "=== dx build --platform web --release --features web (Studio $VERSION, at the root) ==="
rm -rf "$public"
# No --base-path: the bundle is served at the root of its domain.
(cd apps/studio && dx build --platform web --release --features web)

[ -s "$public/index.html" ] || { echo "web bundle: no index.html in $public" >&2; exit 1; }
compgen -G "$public/assets/*.wasm" >/dev/null || { echo "web bundle: no .wasm in $public/assets" >&2; exit 1; }
# Served at the root, the page loads its files from /assets/, never from
# under a path.
if grep -qE '(src|href)="/[^."][^"]*/assets/' "$public/index.html"; then
    echo "web bundle: index.html loads its files from under a path, not the root:" >&2
    grep -oE '(src|href)="[^"]*"' "$public/index.html" >&2
    exit 1
fi

sha="$(git rev-parse HEAD)"
printf '{"version":"%s","commit":"%s"}\n' "$VERSION" "$sha" > "$public/version.json"

mkdir -p "$OUT_DIR"
out="$OUT_DIR/studio-web-$VERSION.tar.gz"
# Reproducible: sorted, no owners, one mtime.
tar -C "$public" --sort=name --owner=0 --group=0 --numeric-owner --mtime=@0 -czf "$out" .
ls -l "$out"
echo "files: $(tar -tzf "$out" | grep -vc '/$')"
