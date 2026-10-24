# studio-web (#217): how the studio-web root (web-bundles.nix,
# `studio-web-root`) is served.
#
# `fts.studioWebSwsConfig root` is static-web-server's config for it, the
# one the preview and the image share:
#
#   - COOP `same-origin` + COEP `require-corp` on everything: the browser
#     keys rig's shared memory needs a cross-origin-isolated page
#     (apps/studio/desktop/src/web_keys_threads.rs);
#   - HTML `no-cache` (nix-store mtimes are 1970: heuristic freshness would
#     pin a page for months); dx's content-hashed `/signal/assets/**` and
#     Session's hash-versioned module and wasm immutable;
#   - `/signal/rigs/**` (the browser keys rig, `/signal/rigs/keys/<profile>`)
#     answered with Signal's page, every other unknown path with the front
#     door.
#
#   just studio-web-preview      # nix run .#studio-web-preview, on :8090
{ ... }:
{
  perSystem = { pkgs, config, ... }:
    let
      studioWebSwsConfig = { root, port ? 8080 }: pkgs.writeText "studio-web-sws.toml" ''
        [general]
        host = "0.0.0.0"
        port = ${toString port}
        root = "${root}"
        page-fallback = "${root}/index.html"
        log-level = "info"
        compression-static = true

        [advanced]
        [[advanced.headers]]
        source = "/**"
        [advanced.headers.headers]
        Cache-Control = "no-cache"
        Cross-Origin-Opener-Policy = "same-origin"
        Cross-Origin-Embedder-Policy = "require-corp"

        [[advanced.headers]]
        source = "/signal/assets/**"
        [advanced.headers.headers]
        Cache-Control = "public, max-age=31536000, immutable"

        # Session's module and wasm: its page asks for them at URLs carrying
        # the wasm's hash (web-bundles.nix, `installSessionApp`), so a URL's
        # bytes never change. The snippets are not versioned and stay on the
        # revalidating default.
        [[advanced.headers]]
        source = "/session/session-daw-web_bg.wasm"
        [advanced.headers.headers]
        Cache-Control = "public, max-age=31536000, immutable"

        [[advanced.headers]]
        source = "/session/session-daw-web.js"
        [advanced.headers.headers]
        Cache-Control = "public, max-age=31536000, immutable"

        [[advanced.rewrites]]
        source = "/signal/rigs/**"
        destination = "/signal/index.html"
      '';

      # The tree on this machine, as the image serves it.
      studio-web-preview = pkgs.writeShellApplication {
        name = "studio-web-preview";
        runtimeInputs = [ pkgs.static-web-server ];
        text = ''
          echo "studio-web: http://localhost:8090  (/, /signal/, /session/)" >&2
          exec static-web-server --config-file ${studioWebSwsConfig {
            root = config.packages.studio-web-root;
            port = 8090;
          }}
        '';
      };
    in
    {
      fts.studioWebSwsConfig = studioWebSwsConfig;
      packages = { inherit studio-web-preview; };
    };
}
