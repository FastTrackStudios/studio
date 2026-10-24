# studio-web (#217): the studio-web root (web-bundles.nix,
# `studio-web-root`) as the image studio.fasttrackstudio.app runs, and the
# preview that serves it the same way.
#
# `fts.studioWebSwsConfig { root, port }` is static-web-server's config:
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
# The start step (scripts/studio-web/serve.sh) serves a runtime directory
# of symlinks into the root plus the `/config.json` it writes from
# TASK_URL / AUTH_URL (scripts/studio-web/write-config.sh): the root is in
# the read-only store, the config is the container's.
#
#   just studio-web-preview                    # on :8090, same origin
#   TASK_URL=https://task.fasttrackstudio.app just studio-web-preview
#   $(nix build --print-out-paths .#studio-web-image) | docker load    # Linux
#   docker run --rm -p 8080:8080 -e TASK_URL=… studio-web:latest
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }:
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

      # The root as the image serves it: the tree, plus version.json (which
      # commit, which release) for the probes and for `deploy`'s "Verify
      # live". Symlinks, not a copy: static-web-server follows them, and only
      # this cheap layer depends on the rev.
      servedRoot = pkgs.runCommand "studio-web-served-root" { } ''
        mkdir -p $out
        for e in ${config.packages.studio-web-root}/*; do ln -s "$e" "$out/$(basename "$e")"; done
        echo '{"rev":"${config.fts.buildRev}","tag":"${config.fts.buildTag}","date":"${config.fts.buildDate}"}' > $out/version.json
      '';

      write-config = pkgs.writeShellApplication {
        name = "studio-web-write-config";
        runtimeInputs = [ pkgs.jq ];
        text = builtins.readFile ../../../scripts/studio-web/write-config.sh;
      };

      serve = pkgs.writeShellApplication {
        name = "studio-web-serve";
        runtimeInputs = [ pkgs.static-web-server pkgs.coreutils pkgs.gnused ];
        runtimeEnv.STUDIO_WEB_WRITE_CONFIG = lib.getExe write-config;
        text = builtins.readFile ../../../scripts/studio-web/serve.sh;
      };

      # The tree on this machine, served as the image serves it (TASK_URL /
      # AUTH_URL from the environment, like the container).
      studio-web-preview = pkgs.writeShellApplication {
        name = "studio-web-preview";
        runtimeInputs = [ pkgs.coreutils ];
        text = ''
          export STUDIO_WEB_RUNTIME_DIR="''${STUDIO_WEB_RUNTIME_DIR:-$(mktemp -d)}"
          echo "studio-web: http://localhost:8090  (/, /signal/, /session/, /config.json)" >&2
          exec ${lib.getExe serve} ${servedRoot} ${studioWebSwsConfig { root = "@root@"; port = 8090; }}
        '';
      };

      # The config writer's cases, as a flake check (`nix flake check`, or
      # `just studio-web-config-test` without nix).
      studio-web-config-test = pkgs.runCommand "studio-web-config-test"
        { nativeBuildInputs = [ pkgs.bash pkgs.jq ]; } ''
        bash ${../../../scripts/studio-web}/write-config-test.sh
        touch $out
      '';
    in
    {
      fts.studioWebSwsConfig = studioWebSwsConfig;
      packages = { inherit studio-web-preview; }
      // lib.optionalAttrs pkgs.stdenv.isLinux {
        # ghcr.io/fasttrackstudios/studio-web (images.yml). Pure nix, no
        # Dockerfile; streamLayeredImage is an EXECUTABLE that streams a
        # docker-archive to stdout (pipe it into `docker load` or skopeo).
        studio-web-image = pkgs.dockerTools.streamLayeredImage {
          name = "studio-web";
          tag = "latest";
          contents = [ pkgs.cacert ];
          # Where the start step writes: /run/studio-web, writable by the
          # unprivileged user it runs as (a read-only root filesystem
          # mounts an emptyDir there: deploy/chart/studio).
          extraCommands = ''
            mkdir -p run/studio-web tmp
            chmod 1777 run run/studio-web tmp
          '';
          config = {
            Entrypoint = [ (lib.getExe serve) "${servedRoot}" "${studioWebSwsConfig { root = "@root@"; }}" ];
            User = "65534:65534";
            ExposedPorts = { "8080/tcp" = { }; };
            Env = [
              "STUDIO_WEB_RUNTIME_DIR=/run/studio-web"
              "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            ];
            # What `scripts/ghcr-images.sh check` reads: the image is the
            # release commit's.
            Labels = {
              "org.opencontainers.image.revision" = config.fts.buildRev;
              "org.opencontainers.image.version" = lib.removePrefix "v" config.fts.buildTag;
              "org.opencontainers.image.source" = "https://github.com/FastTrackStudios/studio";
              "org.opencontainers.image.title" = "studio-web";
              "org.opencontainers.image.description" = "Studio's web front end: the front door, Signal at /signal/, Session at /session/";
              "org.opencontainers.image.licenses" = "GPL-3.0-or-later";
            };
          };
        };
      };
      checks = { inherit studio-web-config-test; };
    };
}
