# Studio's dev environment (devenv 2.4): `devenv shell`, or direnv through
# .envrc. devenv-fleet's modules (devenv.yaml) give the toolchain (rust
# 1.99.1, the rust-toolchain.toml pin), mold, cargo-hakari, cargo-rail, prek,
# just, `fleet-doctor`, dx, binaryen, wasm-bindgen, tailwindcss, the iOS
# linkers, and kache as RUSTC_WRAPPER when kache is on PATH. This file adds
# what the workspace links against, runs and records. It is the flake's
# devShell (nix/modules/shells/default.nix, toolchain.nix) without what the
# fleet provides. CI still enters the flake's `.#ci` shell.
{ pkgs, lib, inputs, ... }:
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
in
{
  packages = with pkgs; [
    cargo-watch
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
  };

  # kache is not set here: the rust module exports RUSTC_WRAPPER from PATH.
  enterShell = ''
    # The flake's shellHook sourced .env; this does the same, into this
    # shell only. Never devenv's dotenv: that copies .env into the store.
    if [ -f "$DEVENV_ROOT/.env" ]; then set -a; . "$DEVENV_ROOT/.env"; set +a; fi

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

  fleet.banner = ''
    FastTrackStudio dev shell
      cargo check --workspace
      cargo run -p studio-desktop   (THE app; --engine = headless rig)
  '';
}
