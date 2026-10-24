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
# The start step (scripts/studio-web/serve.sh) writes `<root>/config/
# config.json` from TASK_URL / AUTH_URL (scripts/studio-web/write-config.sh)
# and `/config.json` is a rewrite to it. The tree is a real copy (`/srv` in
# the image), not a symlink farm into the store: static-web-server refuses
# any file or symlink that resolves outside its root, so the one writable
# directory, `config/`, is inside it too.
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

        # Written when the container starts (scripts/studio-web/serve.sh).
        [[advanced.rewrites]]
        source = "/config.json"
        destination = "/config/config.json"
      '';

      # What a served root holds beside the tree: version.json (which
      # commit, which release) for the probes. Only this cheap file depends
      # on the rev.
      versionJson = pkgs.writeText "studio-web-version.json"
        ''{"rev":"${config.fts.buildRev}","tag":"${config.fts.buildTag}","date":"${config.fts.buildDate}"}'';
      # `installRoot <dir>`: the tree copied into <dir> (real files, the .br
      # alongside), version.json, and the writable config/.
      installRoot = ''
        installRoot() {
          mkdir -p "$1"
          cp -R ${config.packages.studio-web-root}/. "$1/"
          chmod -R u+w "$1"
          cp ${versionJson} "$1/version.json"
          mkdir -p "$1/config"
        }
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
      # AUTH_URL from the environment, like the container), from a copy in
      # a temporary directory.
      studio-web-preview = pkgs.writeShellApplication {
        name = "studio-web-preview";
        runtimeInputs = [ pkgs.coreutils pkgs.gnused ];
        text = ''
          dir="$(mktemp -d)"
          trap 'rm -rf "$dir"' EXIT
          ${installRoot}
          installRoot "$dir/root"
          sed "s|@root@|$dir/root|g" ${studioWebSwsConfig { root = "@root@"; port = 8090; }} > "$dir/sws.toml"
          echo "studio-web: http://localhost:8090  (/, /signal/, /session/, /config.json)" >&2
          ${lib.getExe serve} "$dir/root" "$dir/sws.toml"
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
          # The tree at /srv, real files. /srv/config is where the start
          # step writes, writable by the unprivileged user it runs as (with a
          # read-only root filesystem, mount a tmpfs or emptyDir there:
          # deploy/chart/studio, deploy/docker-compose.yml).
          extraCommands = ''
            ${installRoot}
            installRoot srv
            chmod 1777 srv/config
            mkdir -p tmp
            chmod 1777 tmp
          '';
          config = {
            Entrypoint = [ (lib.getExe serve) "/srv" "${studioWebSwsConfig { root = "/srv"; }}" ];
            User = "65534:65534";
            ExposedPorts = { "8080/tcp" = { }; };
            Env = [
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
