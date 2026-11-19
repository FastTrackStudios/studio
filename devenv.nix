# Studio's dev environment (devenv 2.4): `devenv shell`, or direnv through
# .envrc. devenv-fleet's modules (devenv.yaml) give the toolchain (rust
# 1.99.1, the rust-toolchain.toml pin), mold, cargo-hakari, cargo-rail, prek,
# just, `fleet-doctor`, dx, binaryen, wasm-bindgen, tailwindcss, the iOS
# linkers, and kache as RUSTC_WRAPPER when kache is on PATH. This file adds
# what the workspace links against, runs and records. It is the flake's
# devShell (nix/modules/shells/default.nix, toolchain.nix) without what the
# fleet provides. CI enters this same shell: `devenv shell -- <cmd>`.
{ pkgs, lib, inputs, config, ... }:
let
  # The nixpkgs-dx pin the fleet's dioxus module builds dx from. Playwright
  # and linuxdeploy come from the same set.
  pkgsDx = import inputs.nixpkgs-dx { inherit (pkgs.stdenv) system; };

  nightlyDate = "2026-04-05";

  # cargo-nightly — an explicit door to a nightly with rust-src and wasm, for
  # the one build stable cannot do: rig's shared-memory worklet (wasm threads
  # need -Z build-std, `just keys-worklet-wasm-threads`). Reached by name,
  # never on PATH as `cargo`, so the stable 1.99.1 pin governs every other build.
  rustNightlyWasm = pkgs.rust-bin.nightly.${nightlyDate}.minimal.override {
    extensions = [ "rust-src" ];
    targets = [ "wasm32-unknown-unknown" ];
  };
  cargoNightly = pkgs.writeShellScriptBin "cargo-nightly" ''
    export RUSTC="${rustNightlyWasm}/bin/rustc"
    exec ${rustNightlyWasm}/bin/cargo "$@"
  '';

  # phon-jit's copy-and-patch stencils chain via `become`, a nightly feature.
  # Darwin only: the native stencil backend compiles only on macOS-aarch64.
  rustNightly = pkgs.rust-bin.nightly.${nightlyDate}.minimal;

  # tracey — spec coverage (`just daw-flows`). Not in nixpkgs, and `cargo
  # install` stalls on its node build, so this is the upstream release binary
  # (nix/modules/tracey.nix).
  traceyVersion = "1.3.0";
  traceyAsset = {
    x86_64-linux = {
      target = "x86_64-unknown-linux-gnu";
      hash = "sha256-+8DCXEQyjMsJcLJQkJX/KUEvpyy7xrADbEzujBKCH0c=";
    };
    aarch64-linux = {
      target = "aarch64-unknown-linux-gnu";
      hash = "sha256-fmTjbT1LXr0j5tv8sWDCitgacZBkGdM2u+uOFG6CYGQ=";
    };
    aarch64-darwin = {
      target = "aarch64-apple-darwin";
      hash = "sha256-NltLMFbFiZAVJrVEAbI2NEMKkl/LEyf1zW9TVoY7INU=";
    };
  }.${pkgs.stdenv.hostPlatform.system} or null;
  tracey = pkgs.stdenvNoCC.mkDerivation {
    pname = "tracey";
    version = traceyVersion;
    src = pkgs.fetchurl {
      url = "https://github.com/bearcove/tracey/releases/download/v${traceyVersion}/tracey-${traceyAsset.target}.tar.xz";
      inherit (traceyAsset) hash;
    };
    sourceRoot = "tracey-${traceyAsset.target}";
    nativeBuildInputs = lib.optionals pkgs.stdenv.isLinux [ pkgs.autoPatchelfHook ];
    buildInputs = lib.optionals pkgs.stdenv.isLinux [ pkgs.stdenv.cc.cc.lib ];
    dontConfigure = true;
    dontBuild = true;
    installPhase = ''
      runHook preInstall
      install -Dm755 tracey "$out/bin/tracey"
      runHook postInstall
    '';
    meta.mainProgram = "tracey";
  };

  # The desktop crates' native libraries, on LD_LIBRARY_PATH so a test binary
  # runs from this shell. libstdc++ too: Luau (architect-scripting) is C++.
  linuxLibPath = lib.makeLibraryPath (with pkgs; [
    stdenv.cc.cc.lib
    fontconfig
    freetype
    openssl
    alsa-lib
    libjack2
    pipewire
    libGL
    vulkan-loader
    gtk3
    glib
    gdk-pixbuf
    pango
    cairo
    atk
    libx11
    libxcb
    libxkbcommon
    wayland
    webkitgtk_4_1
    libsoup_3
    xdotool
  ]);

  # The REAPER the integration suites run against: the wrapped build `nix run
  # .#fts-reaper` uses (nix/modules/packages/reaper.nix). libxml2_13 is the
  # soname SWELL dlopens; an unwrapped REAPER fails with a GDK assertion about
  # a missing display, which reads as a broken Xvfb.
  reaper = pkgs.callPackage ./nix/vendor/reaper-flake/wrapper/reaper/pkgs/reaper.nix {
    jackLibrary = pkgs.pipewire.jack;
    libxml2 = pkgs.libxml2_13;
  };
in
{
  packages = with pkgs; [
    cargo-watch
    # The local stack's web app, its front and its vault sync
    # (scripts/local-stack.sh `run web`, `run front`, `sync`).
    static-web-server
    caddy
    rsync
    cargo-nextest
    bacon
    flac
    # uv — bootstraps graphify in enterShell (a PyPI tool, not in nixpkgs).
    uv
    shellcheck
    actionlint
    # Native headers and build tools for build scripts: openssl-sys, cmake,
    # python3 for stylo's generated Rust, libclang for bindgen.
    openssl
    openssl.dev
    libiconv
    fontconfig
    freetype
    cmake
    python3
    # cargo fetches every git dependency through the git CLI. On macOS this
    # real git shadows the /usr/bin/git xcrun shim.
    git
    # Sets bindgen's clang arguments for crates that run it (pipewire-sys).
    rustPlatform.bindgenHook
    cargoNightly
  ]
  ++ lib.optional (traceyAsset != null) tracey
  ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
    # pw-jack — run the live rigs through PipeWire's JACK shim; measure
    # round-trip latency with jack_iodelay / jack_lsp.
    pipewire.jack
    jack-example-tools
    # Patchbay: the session manager and pulse tools its sandboxed tests drive.
    wireplumber
    pulseaudio
    # Xvfb + xvfb-run: virtual display for the REAPER integration tests.
    xorg.xorgserver
    xvfb-run
    # cargo-sweep reclaims stale target/ artifacts (`just sweep`).
    cargo-sweep
    # dx's AppImage bundler (`dx bundle --package-types appimage`) shells out
    # to linuxdeploy, which is not in the fleet nixpkgs.
    pkgsDx.linuxdeploy
    # Native libraries the crates link against (the flake's buildInputs).
    alsa-lib
    alsa-lib.dev
    # hidapi's build script pkg-configs libudev (kontrol's raw USB HID).
    udev
    # ort (load-dynamic) dlopens libonnxruntime at run time (session-guide/tts).
    onnxruntime
    # cpal's jack and pipewire features (signal-sampler).
    libjack2
    pipewire
    pipewire.dev
    glib
    gtk3
    gdk-pixbuf
    pango
    cairo
    atk
    harfbuzz
    libsoup_3
    webkitgtk_4_1
    xdotool
    libx11
    libxcursor
    libxrandr
    libxi
    libxcb
    libxkbcommon
    wayland
    libGL
    vulkan-loader
  ]);

  env = {
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
    OPENSSL_DIR = "${pkgs.openssl.dev}";
    OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
    # The editor web suite's Playwright browsers (libs/editor/tests). The
    # playwright-driver here (1.59.1) matches the @playwright/test pin in
    # tests/package.json: keep them in lockstep. Without this Playwright falls
    # back to its own chromium, which cannot load shared libs on NixOS.
    PLAYWRIGHT_BROWSERS_PATH = "${pkgsDx.playwright-driver.browsers}";
    PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
  }
  // lib.optionalAttrs pkgs.stdenv.isLinux {
    LD_LIBRARY_PATH = linuxLibPath;
    ORT_DYLIB_PATH = "${pkgs.onnxruntime}/lib/libonnxruntime.so";
    XDG_DATA_DIRS = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}";
    # WebKitGTK's accelerated compositing fails on NixOS (GBM buffer error,
    # white window). https://github.com/NixOS/nixpkgs/issues/32580
    WEBKIT_DISABLE_COMPOSITING_MODE = "1";
  }
  // lib.optionalAttrs pkgs.stdenv.isDarwin {
    PHON_JIT_NIGHTLY_RUSTC = "${rustNightly}/bin/rustc";
    # libz-sys asks pkg-config for zlib, and pkg-config answers for the iOS
    # targets too: nixpkgs' zlib, built for macOS, which an iOS link refuses
    # ("building for 'iOS', but linking in object file … built for 'macOS'";
    # studio#444, every TestFlight build since architect's workspace-hack
    # turned on flate2's `zlib`). Without pkg-config it links the SDK's own
    # zlib (`-lz`), the iPhoneOS SDK's for iOS and the macOS SDK's here.
    ZLIB_NO_PKG_CONFIG = "1";
  };

  # kache is not set here: the rust module exports RUSTC_WRAPPER from PATH.
  enterShell = ''
    # The flake's shellHook sourced .env; this does the same, into this
    # shell only. Never devenv's dotenv: that copies .env into the store.
    if [ -f "$DEVENV_ROOT/.env" ]; then set -a; . "$DEVENV_ROOT/.env"; set +a; fi

    # The CI shell's software Vulkan: CI has no GPU, so wgpu takes lavapipe as
    # its adapter and the frame tests run. Only on CI; a dev shell keeps the
    # machine's GPU.
    ${lib.optionalString pkgs.stdenv.isLinux ''
      if [ -n "''${CI:-}" ]; then
        export VK_DRIVER_FILES="${pkgs.mesa}/share/vulkan/icd.d/lvp_icd.${pkgs.stdenv.hostPlatform.uname.processor}.json"
        export LP_NUM_THREADS=2
      fi
    ''}

    # Store-provided tools (dx, wasm-bindgen) must win over stale cargo-installed
    # copies, so cargo's and uv's bins are appended, not prepended.
    export PATH="$PATH:$HOME/.cargo/bin:$HOME/.local/bin"

    # graphify — the repo knowledge graph (`just graph`). Code extraction is
    # local (tree-sitter). Dev-only: CI skips the installer.
    if [ -z "''${CI:-}" ]; then
      GRAPHIFY_VERSION="0.9.15"
      if ! uv tool list 2>/dev/null | grep -q "graphifyy v$GRAPHIFY_VERSION"; then
        UV_PYTHON_DOWNLOADS=never uv tool install --force \
          --python "${pkgs.python3}/bin/python3" \
          "graphifyy[mcp]==$GRAPHIFY_VERSION" >/dev/null 2>&1 \
          || echo "  (graphify install failed — run: uv tool install 'graphifyy[mcp]==$GRAPHIFY_VERSION')" >&2
      fi
    fi
  '';

  # `devenv --profile reaper shell`: the REAPER integration suites
  # (`#[reaper_test]`, `just reaper-test`). What the flake's `.#reaper-test`
  # shell adds to the default env, which already has Xvfb, xvfb-run, xdotool,
  # PipeWire's JACK shim and cargo-nextest.
  # `devenv --profile wasi shell`: the fleet toolchain plus `wasm32-wasip2`,
  # what Studio's island builds for (apps/architect/island, `just island`,
  # #445), and node for Architect's `build-web` (its JSPI needs the current
  # release). A profile, as Architect's: an extra target changes the rustc
  # store path, so kache shares nothing with the default shell's builds.
  profiles.wasi.module = {
    fleet.rust.extraTargets = [ "wasm32-wasip2" ];
    packages = [ pkgs.nodejs_latest ];
  };

  profiles.reaper.module.packages = lib.optionals pkgs.stdenv.isLinux [
    reaper
    pkgs.openbox      # a bare Xvfb leaves REAPER's restored dialogs unmanaged
    pkgs.imagemagick  # `import` and `identify`: screenshots, window lookup
    pkgs.xwininfo     # the window geometry daw::test::VirtualDisplay reads
  ];

  # `devenv up`: the local stack (docs/local-stack.md), complete, rebuilt on
  # a save — each `scripts/studio-dev.sh run <name>`:
  #   engine  Studio's headless engine (`studio engine`): every window
  #           attaches to it, so a rebuild never takes the audio down.
  #   studio  Studio: Studio Architect (Architect with Studio's extensions,
  #           Lighting and the DAW view linked in) under `dx serve`; the
  #           server too (STUDIO_SERVE: its node in-process, holding a copy
  #           of the test vault and its share link).
  #   web     Studio Web: Architect's web app under `dx serve --platform
  #           web`, proxying the node's paths (/s, /vox, /org, /node, /_ext)
  #           to Studio, so a share link opens on its origin.
  # `studio-desktop-legacy` (off) is the old Studio window, until the merge
  # into Studio Architect lands: `devenv up engine studio-desktop-legacy`,
  # or `just dev` with it named. Which vault, setlist, share link, set and
  # profile, and whether Architect and Keyflow are local: local-stack.toml
  # under this machine's gitignored local-stack.local.toml. `just up` runs
  # this as a systemd user unit.
  processes = let
    root = config.devenv.root;
    dev = "${root}/scripts/studio-dev.sh";
    probe = name: { exec = "${dev} ready ${name}"; period = 2; };
    # .env (gitignored, secrets) into the process, never through devenv's
    # dotenv (which copies it into the store).
    run = name: ''
      set -a; [ -f "${root}/.env" ] && . "${root}/.env"; set +a
      exec ${dev} run ${name}
    '';
  in {
    engine = {
      exec = run "engine";
      ready = probe "engine";
    };
    studio = {
      exec = run "studio";
      after = [ "devenv:processes:engine" ];
      ready = probe "studio";
    };
    web = {
      exec = run "web";
      ready = probe "web";
    };
    studio-desktop-legacy = {
      exec = run "studio-desktop-legacy";
      after = [ "devenv:processes:engine" ];
      start.enable = false;
    };
  };

  # `devenv --profile legacy-stack up` (`just dev-legacy`, `just demo`,
  # `just website`): the stack before Studio served itself —
  # Architect's issuer, Studio's headless node, the static web app, the
  # front (Caddy) that puts both on one origin, the site, each a process
  # (scripts/local-stack.sh). Kept until nothing needs it.
  profiles.legacy-stack.module = let
    root = config.devenv.root;
    stack = "${root}/scripts/local-stack.sh";
    probe = name: { exec = "${stack} ready ${name}"; period = 2; };
    run = name: ''
      set -a; [ -f "${root}/.env" ] && . "${root}/.env"; set +a
      exec ${stack} run ${name}
    '';
  in {
    processes = {
      mock-oauth = { exec = run "mock-oauth"; ready = probe "mock-oauth"; };
      auth = {
        exec = run "auth";
        after = [ "devenv:processes:mock-oauth" ];
        ready = probe "auth";
      };
      node = {
        exec = run "node";
        after = [ "devenv:processes:auth" ];
        ready = probe "node";
      };
      legacy-web = {
        exec = run "web";
        after = [ "devenv:processes:auth" ];
        ready = probe "web";
      };
      front = { exec = run "front"; ready = probe "front"; };
      site.exec = run "site";
      # The default profile's, off here: `devenv --profile legacy-stack up`
      # alone is the legacy stack.
      engine.start.enable = false;
      studio.start.enable = false;
      web.start.enable = false;
    };
  };

  fleet.banner = ''
    FastTrackStudio dev shell
      cargo check --workspace
      cargo run -p studio-desktop   (THE app; --engine = headless rig)
  '';
}
