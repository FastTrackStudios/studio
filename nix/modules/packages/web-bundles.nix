# dx web bundles (signal-web, session-web, session-app-web) + their
# static-site OCI images.
# dx bundle → $out/www (+ brotli pre-compression).
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }:
    let
      inherit (config.fts) craneLib commonArgs mkStaticSite;

      # Shared env for the dx web-bundle builds: arborium /
      # arborium-tree-sitter compile the tree-sitter C runtime +
      # grammars to wasm via cc::Build — needs an unwrapped clang
      # targeting wasm32 and llvm-ar (the cc-wrapper injects
      # host-only hardening flags clang rejects for wasm). Without
      # these the C symbols stay as unresolved `(import "env" ...)`
      # entries and the shipped bundle white-screens. Mirrors
      # devShells.default exactly.
      dxWebEnv = {
        CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang-unwrapped}/bin/clang";
        AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools-unwrapped}/bin/llvm-ar";
        CFLAGS_wasm32_unknown_unknown = "-isystem ${pkgs.llvmPackages_18.clang}/resource-root/include";
        # Hermetic dx: no network in the sandbox. With NO_DOWNLOADS
        # set, dx resolves wasm-opt / wasm-bindgen from PATH instead
        # of fetching them from GitHub.
        NO_DOWNLOADS = "1";
      };

      dxWebNativeInputs = commonArgs.nativeBuildInputs ++ [
        config.fts.dx.cli          # dx 0.7.9 (nixpkgs-dx)
        config.fts.dx.wasmBindgen  # 0.2.126, matches the lock
        config.fts.dx.binaryen     # wasm-opt 129 — what dx 0.7.9 pins
      ] ++ (with pkgs; [
        tailwindcss_4
        # Resolves architect-ui's git-dep checkout for the tailwind
        # @source globs (session-web's preBuild).
        jq
        # Pre-compression for --compression-static serving.
        brotli
        # stylo (Blitz's CSS engine, in the Session app's page) generates
        # its property tables with a Python script at build time.
        python3
        llvmPackages_18.clang-unwrapped
        llvmPackages_18.bintools-unwrapped
      ]);

      # A bundle's dependencies, compiled once and kept. crane builds them
      # from the manifests and the lockfile alone (`buildDepsOnly`'s dummy
      # source), so this derivation changes only when a dependency does:
      # a release finds it in the runner's store instead of compiling every
      # crate again, and the bundle compiles only the workspace's own.
      # Its flags must be the bundle's (profile, target, features,
      # RUSTFLAGS, `--config` overrides): cargo reuses an artifact only
      # when its fingerprint matches, and rebuilds the rest (slower, never
      # wrong).
      wasmDeps = { pname, profile, cargoExtraArgs, env ? { } }:
        craneLib.buildDepsOnly (commonArgs // dxWebEnv // env // {
          inherit pname;
          version = "0.1.0";
          nativeBuildInputs = dxWebNativeInputs;
          CARGO_PROFILE = profile;
          inherit cargoExtraArgs;
          # Only the build: a check or a test run would compile other
          # profiles nobody reuses.
          cargoCheckCommand = "true";
          doCheck = false;
        });

      # The dx build runs from the app dir but writes to the
      # WORKSPACE-ROOT target/dx/<name>/release/web/public.
      mkDxWebBundle = { pname, appDir, dxName, preBuild ? "", dxArgs ? "", env ? { }, extraNativeBuildInputs ? [ ], cargoArtifacts ? null }:
        craneLib.buildPackage (commonArgs // dxWebEnv // env // {
          inherit pname cargoArtifacts;
          version = "0.1.0";
          cargoExtraArgs = "--manifest-path ${appDir}/Cargo.toml";
          nativeBuildInputs = dxWebNativeInputs ++ extraNativeBuildInputs;
          doNotPostBuildInstallCargoBinaries = true;
          buildPhaseCargoCommand = ''
            export HOME="$TMPDIR/dx-home"
            mkdir -p "$HOME"
            ${preBuild}
            cd ${appDir}
            # --debug-symbols false: drop DWARF for a smaller release
            # bundle (and it sidesteps DWARF-version mismatches in
            # wasm-opt).
            dx build --release --platform web --debug-symbols false ${dxArgs}
          '';
          # buildPhase ends inside ${appDir}; anchor the copy at the
          # workspace root explicitly.
          installPhaseCommand = ''
            mkdir -p $out/www
            srcdir="$(pwd)"
            case "$srcdir" in */${appDir}) srcdir="''${srcdir%/${appDir}}";; esac
            cp -R "$srcdir/target/dx/${dxName}/release/web/public/." $out/www/
            # Pre-compress text/wasm so static-web-server's
            # --compression-static serves .br variants (the multi-MB
            # wasm goes over the wire at brotli size).
            find $out/www -type f \( -name '*.wasm' -o -name '*.js' \
              -o -name '*.css' -o -name '*.html' -o -name '*.json' \
              -o -name '*.svg' \) -exec brotli --keep --quality=9 {} +
          '';
          doCheck = false;
        });

      # The public demo's set: a live share link (`task share live <setlist>
      # --reset-minutes 5`) on the public Task, baked into the site's /demo —
      # days-to-praise's Worship Set, starting over every five minutes.
      sessionDemoLink = "https://task.fasttrackstudio.app/org/days-to-praise/share/1e75d7fa11c540ac8dbbd461048e59fc";

      # The Session app in a browser (`just web-daw`): what /demo and every
      # live link open, served under the site's /app/. A plain cargo +
      # wasm-bindgen build (no dx: the page is a library host, see
      # session-daw's web_host).
      # SIMD: vello's painting (fearless_simd) picks its level at compile
      # time and runs its scalar fallback without `+simd128`. The same
      # RUSTFLAGS on the deps, or their fingerprints miss (`just web-daw`
      # sets them too).
      sessionAppWebRustflags = "-C target-feature=+simd128";
      session-app-web-deps = wasmDeps {
        pname = "session-app-web";
        profile = "wasm-release";
        cargoExtraArgs = "--locked -p session-daw-web --target wasm32-unknown-unknown";
        env.RUSTFLAGS = sessionAppWebRustflags;
      };
      session-app-web = craneLib.buildPackage (commonArgs // dxWebEnv // {
        pname = "session-app-web";
        RUSTFLAGS = sessionAppWebRustflags;
        version = "0.1.0";
        # The open-a-set page's "try the live demo" opens it in the app
        # itself (`session::open_link::demo_href`), wherever it is served.
        SESSION_DEMO_LINK = sessionDemoLink;
        cargoArtifacts = session-app-web-deps;
        nativeBuildInputs = dxWebNativeInputs;
        doNotPostBuildInstallCargoBinaries = true;
        buildPhaseCargoCommand = ''
          # `wasm-release`: size-optimized with whole-program LTO, the
          # audio path at full speed (see the profile in Cargo.toml).
          cargo build --profile wasm-release -p session-daw-web --target wasm32-unknown-unknown
          mkdir -p dist
          wasm-bindgen --target web --no-typescript --out-dir dist \
            target/wasm32-unknown-unknown/wasm-release/session-daw-web.wasm
          wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
            --enable-sign-ext --enable-mutable-globals --enable-reference-types \
            --enable-multivalue --enable-simd dist/session-daw-web_bg.wasm -o dist/session-daw-web_bg.wasm
        '';
        installPhaseCommand = ''
          mkdir -p $out/www
          cp -R dist/. $out/www/
          cp apps/session/daw-web/www/index.html $out/www/
          # The best brotli there is: done once per build, and the wasm
          # comes out 6.4 MB against 7.5 at Cloudflare's own level.
          find $out/www -type f \( -name '*.wasm' -o -name '*.js' -o -name '*.html' \) \
            -exec brotli --keep --quality=11 --lgwin=24 {} +
        '';
        doCheck = false;
      });

      # task-webapp moved to the task repo with the August 2026 split,
      # along with task-server and the ui-lab bundle.

      # signal.fasttrackstudio.app — the Signal landing page, rig demos
      # and guide (`just signal-web-build`). No tailwind step:
      # assets/site.css is committed and inlined by the app.
      #
      # (This replaced an `fts-site-web` bundle of `apps/site` for
      # fasttrackstudio.app — a directory that left with the August 2026
      # split, so that derivation referred to nothing.)
      signal-web = mkDxWebBundle {
        pname = "signal-web";
        appDir = "apps/signal/web";
        dxName = "signal-web";
        # --wasm-split is NOT optional here, and not really about splitting.
        #
        # Without it, wasm-opt (binaryen 129) aborts with SIGABRT and no
        # diagnostic on this bundle; dx reports the failure, carries on, and
        # ships the UNOPTIMISED wasm — 2,029,954 bytes. With it, wasm-opt
        # completes and the same bundle is 859,778. Measured both ways on a
        # clean target dir. The route splitting itself earns almost nothing;
        # do not drop the flag as "we don't need splitting" — that silently
        # triples the bundle.
        #
        # `--ssg` pre-renders the guide (`signal_web::static_routes`) into
        # the same `public` directory; `--force-sequential` is REQUIRED
        # alongside it (the pre-render borrows the client build's
        # `public/index.html` as its page shell — dioxus#3518).
        dxArgs = "--ssg --force-sequential --wasm-split --features wasm-split";
        env = {
          # `dx build --platform web` still compiles a HOST binary (the
          # SSG server), and signal-web depends on the rig UI crates,
          # which reach cpal -> {alsa,jack,pipewire}-sys: the host half
          # needs the devshell's native libraries even though the shipped
          # artifact is wasm.
          buildInputs = config.fts.buildInputs;
          # The rig's NAM captures (rig-models, `just assets`) are not in
          # the source: nothing here ships a rig, so the guitar crate's
          # release check for them stands down (features/rigs/guitar/build.rs).
          SIGNAL_RIG_NO_CAPTURES = "1";
        };
        # pipewire's -sys crates (cpal, in that host half) run bindgen:
        # libclang and the C headers, as the devshell gives them.
        extraNativeBuildInputs = [ pkgs.rustPlatform.bindgenHook ];
      };

      # session.fasttrackstudio.app — landing page + the live in-browser
      # setlist demo (`just web`). apps/session/web/tailwind.css @sources
      # architect-ui, a GIT DEP with no stable path on disk — resolve it
      # via cargo metadata and symlink it in first, the same way
      # `just _web-tw-link` does locally. session-ui is a local path dep
      # (features/session/ui) and needs no symlink.
      session-web = mkDxWebBundle {
        pname = "session-web";
        appDir = "apps/session/web";
        dxName = "session-web";
        preBuild = ''
          mkdir -p apps/session/web/.tailwind-src
          for crate in architect-ui; do
            dir="$(cargo metadata --format-version 1 --offline \
              | jq -er --arg c "$crate" \
                  '.packages[] | select(.name == $c) | .manifest_path' \
              | head -1)"
            dir="''${dir%/Cargo.toml}"
            if [ -z "$dir" ] || [ ! -d "$dir" ]; then
              echo "cannot resolve $crate for the tailwind @source globs" >&2
              exit 1
            fi
            ln -sfn "$dir" "apps/session/web/.tailwind-src/$crate"
          done
          (cd apps/session/web && tailwindcss -i ./tailwind.css -o ./assets/tailwind.css --minify)
        '';
        # `--ssg` pre-renders the guide into the same `public` directory
        # the bundle lands in: dx builds the app's server too, runs it,
        # asks it for `static_routes` and requests each. Nothing deploys
        # that server — what ships is still a directory of static files.
        #
        # `--fullstack` because dx decides whether to build a server from
        # the CLIENT's features, and `dioxus/fullstack` is on the
        # `server` feature alone (its reqwest would be a second major in
        # the wasm binary). Without the flag there is no server target
        # and `--ssg` silently does nothing.
        #
        # `--force-sequential` because the pre-render borrows
        # `public/index.html` for its page shell and the client build
        # writes that file; in parallel the pages can come out in
        # Dioxus's bare fallback shell, with the build still reporting
        # success. (dioxus#3518.)
        dxArgs = "--ssg --fullstack --force-sequential";
        env = {
          SESSION_DEMO_LINK = sessionDemoLink;
          # The pre-render server is a native binary over the same session
          # crates the desktop links (Blitz, baseview's X11 window layer):
          # the desktop's native libraries, not just openssl.
          buildInputs = config.fts.buildInputs ++ [ pkgs.openssl ];
        };
      };

      # What session.fasttrackstudio.app serves: the site, and the app
      # under /app/.
      #
      # The app's page loads its module and wasm at URLs carrying the
      # wasm's hash. Their names never change, and Cloudflare gives `.js`
      # a four-hour browser TTL whatever the origin says: after a deploy a
      # returning browser paired its cached glue with the new wasm, which
      # does not link (a blank page). The page itself is `no-cache`, so a
      # new build's page always asks for its own files.
      #
      # `installSessionApp <dir>`: the app's files into <dir>, its page
      # pointed at them by the wasm's hash. Shared by every tree the app
      # is served from (the session site's /app/, studio-web's /session/).
      installSessionApp = ''
        installSessionApp() {
          local dir="$1" v snippets
          mkdir -p "$dir"
          cp -R ${session-app-web}/www/. "$dir/"
          chmod -R u+w "$dir"
          v=$(sha256sum "$dir/session-daw-web_bg.wasm" | cut -c1-16)
          # The snippets the module imports, preloaded beside it: their
          # directories are named by crate hash, so they are listed here from
          # what was built rather than written into the page.
          snippets=$(cd "$dir" && find snippets -name '*.js' | sort \
            | sed 's|.*|    <link rel="modulepreload" href="./&" />|')
          substituteInPlace "$dir/index.html" \
            --replace-fail '"./session-daw-web.js"' "\"./session-daw-web.js?v=$v\"" \
            --replace-fail 'href="./session-daw-web_bg.wasm"' "href=\"./session-daw-web_bg.wasm?v=$v\"" \
            --replace-fail '    <!-- snippets -->' "$snippets" \
            --replace-fail 'init();' "init({ module_or_path: \"./session-daw-web_bg.wasm?v=$v\" });"
          # Its precompressed copy is the old page: served first, it would
          # undo the above.
          rm -f "$dir/index.html.br" "$dir/index.html.gz"
        }
      '';

      session-site-root = pkgs.runCommand "session-site-root" { } ''
        ${installSessionApp}
        mkdir -p $out
        cp -R ${session-web}/www/. $out/
        installSessionApp $out/app
      '';

      # The keys rig's AudioWorklet bundle (`just keys-worklet-wasm`): the
      # browser keys rig loads it from `<base>/worklet/` (web_keys_rig.rs).
      # Its own derivation because its RUSTFLAGS (4 GB memory, SIMD) are
      # not the page's. The single-threaded build: the shared-memory one
      # needs nightly build-std, and the page falls back without it
      # (web_keys_threads.rs).
      signal-keys-worklet-deps = wasmDeps {
        pname = "signal-keys-worklet";
        profile = "release";
        cargoExtraArgs = "--locked -p web-audio-worklet --features keys --lib --target wasm32-unknown-unknown";
        env.RUSTFLAGS = "-C link-arg=--max-memory=4294967296 -C target-feature=+simd128";
      };
      signal-keys-worklet = craneLib.buildPackage (commonArgs // {
        pname = "signal-keys-worklet";
        version = "0.1.0";
        cargoArtifacts = signal-keys-worklet-deps;
        nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ config.fts.dx.wasmBindgen ];
        doNotPostBuildInstallCargoBinaries = true;
        RUSTFLAGS = "-C link-arg=--max-memory=4294967296 -C target-feature=+simd128";
        buildPhaseCargoCommand = ''
          cargo build -p web-audio-worklet --features keys --lib \
            --target wasm32-unknown-unknown --release
        '';
        installPhaseCommand = ''
          mkdir -p $out/worklet
          wasm-bindgen --target web --out-dir $out/worklet \
            --out-name signal_keys_worklet \
            target/wasm32-unknown-unknown/release/web_audio_worklet.wasm
          for js in keys_processor worklet_polyfill keys_decoder_worker keys_streamer_worker; do
            cp features/adapters/web/worklet/js/$js.js $out/worklet/
          done
        '';
        doCheck = false;
      });

      # Signal in a browser: studio-desktop's web build (the rig remote,
      # the browser keys rig), built to be served under /signal/.
      # `--base-path signal` makes dx write its page's asset URLs under it;
      # the URLs the app writes itself go through `web_base` (the worklet
      # bundle, the `/rigs/keys/{profile}` route).
      studio-signal-web = mkDxWebBundle {
        pname = "studio-signal-web";
        appDir = "apps/studio/desktop";
        dxName = "studio-desktop";
        dxArgs = "--no-default-features --features signal --base-path signal";
        # dx's own cargo flags for a web release: its `wasm-release`
        # profile (the workspace's), the wasm target, no default features,
        # and `strip=false` (dx strips after the asset pass).
        cargoArtifacts = wasmDeps {
          pname = "studio-signal-web";
          profile = "wasm-release";
          cargoExtraArgs = "--locked -p studio-desktop --target wasm32-unknown-unknown --no-default-features --features signal --config profile.wasm-release.strip=false";
          env.SIGNAL_RIG_NO_CAPTURES = "1";
        };
        env = {
          # Nothing here ships a rig's NAM captures (rig-models, `just
          # assets`): the guitar crate's release check stands down
          # (features/rigs/guitar/build.rs).
          SIGNAL_RIG_NO_CAPTURES = "1";
        };
      };

      # studio.fasttrackstudio.app (#217): the front door at /, Signal at
      # /signal/, Session at /session/. Static files and nothing else:
      # Task and fts-auth do every connection (`studio-web-image`).
      studio-web-root = pkgs.runCommand "studio-web-root" { nativeBuildInputs = [ pkgs.brotli ]; } ''
        ${installSessionApp}
        mkdir -p $out/signal
        cp ${../../../apps/studio/web/index.html} $out/index.html
        brotli --keep --quality=11 $out/index.html
        cp -R ${studio-signal-web}/www/. $out/signal/
        chmod -R u+w $out/signal
        cp -R ${signal-keys-worklet}/worklet $out/signal/worklet
        chmod -R u+w $out/signal/worklet
        find $out/signal/worklet -type f \( -name '*.wasm' -o -name '*.js' \) \
          -exec brotli --keep --quality=11 {} +
        installSessionApp $out/session
      '';
    in
    {
      packages = {
        inherit signal-web session-web session-app-web studio-signal-web signal-keys-worklet studio-web-root
          session-app-web-deps signal-keys-worklet-deps;
        studio-web = studio-web-root;
      }
      // lib.optionalAttrs pkgs.stdenv.isLinux {
        signal-web-image = mkStaticSite {
          name = "signal-web";
          siteRoot = "${signal-web}/www";
        };
        session-web-image = mkStaticSite {
          name = "session-web";
          siteRoot = "${session-site-root}";
        };
      };
    };
}
