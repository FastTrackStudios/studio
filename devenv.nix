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
    # Serves a built web app's folder as it is (a release's web build).
    static-web-server
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
    # unshare and ip: the room's test on a network with no route out
    # (daw-extension's tests/room) runs its engines in a namespace of
    # their own with only a local network.
    util-linux
    iproute2
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
    # studio#444, every iOS build since architect's workspace-hack
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
  # what Studio's island builds for (features/island, `just island`,
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

  # `devenv up`: the engine, headless (`just engine`: `daw-extension
  # --engine`, built first), exporting to the telemetry module's collector,
  # which devenv-fleet's `telemetry` import runs beside it (otelcol on
  # :4318, and its viewer). The window is `just studio`, under dx serve, in a
  # terminal of its own: it starts or joins an engine itself. The old local
  # stack (Architect's server, auth, the web app; scripts/local-stack.sh)
  # went with the monorepo. Run it detached (`devenv up -d`) and watch it
  # with `devenv processes attach`.
  processes.engine = {
    exec = "exec just engine";
    env = config.fleet.telemetry.processEnv;
  };

  # Claude Code: devenv writes .mcp.json from this (git-ignored: it names
  # this checkout's path). `devenv`: its own MCP (search nixpkgs and devenv
  # options, https://devenv.sh/mcp/); `studio`: the running Studio's Action
  # List for an agent (scripts/studio-mcp -> features/agent-mcp, on the
  # app's control socket: no port, no token, survives Studio's restarts;
  # apps/studio/docs/mcp.md). Teley's MCP is the user's own
  # (`teley-cli mcp --host localhost:8787`), reading what this stack exports.
  claude.code.enable = true;
  claude.code.mcpServers = {
    devenv = {
      type = "stdio";
      command = "devenv";
      args = [ "mcp" ];
      env = { DEVENV_ROOT = config.devenv.root; };
    };
    studio = {
      type = "stdio";
      command = "${config.devenv.root}/scripts/studio-mcp";
      args = [ ];
    };
  };

  fleet.banner = ''
    FastTrackStudio dev shell
      just studio   the desktop app (apps/studio), with its engine
      just engine   the engine alone, headless
      just check    cargo check --workspace + the layout rules
  '';
}
