# plugins's dev environment (devenv 2.4): `devenv shell`, or direnv through
# .envrc. devenv-fleet's `rust` module (devenv.yaml) gives the toolchain (rust
# 1.99.1 with wasm32, and on macOS the x86_64-apple-darwin target the
# universal plugin bundles need; the rust-toolchain.toml pin), mold,
# cargo-hakari, cargo-rail, prek, just, pkg-config and `fleet-doctor`; its
# `darwin` module the macOS library path. This file adds what the effects,
# their editors and the plugin shells link against: Studio's devenv.nix cut
# to that set. The Linux release job (.github/workflows/release.yml) builds
# in this shell on THEBATTLESHIP, which is NixOS: it has no apt, so every
# library a plugin links on Linux must be here.
{ pkgs, lib, ... }:
let
  linux = with pkgs; lib.optionals stdenv.isLinux [
    # Audio I/O: cpal's ALSA, JACK and PipeWire backends.
    alsa-lib
    libjack2
    pipewire
    # The plugin windows (baseview) and the native editors: X11, Wayland, GL
    # and Vulkan for wgpu.
    libx11
    libxcb
    libxcb-wm # xcb-icccm
    libxcursor
    libxrandr
    libxi
    libxkbcommon
    wayland
    libGL
    vulkan-loader
    # The Dioxus desktop editors (GTK/WebKit).
    gtk3
    glib
    gdk-pixbuf
    pango
    cairo
    atk
    harfbuzz
    libsoup_3
    webkitgtk_4_1
    libayatana-appindicator
    dbus
    xdotool
  ];
  common = with pkgs; [ openssl fontconfig freetype ];
in
{
  packages = common ++ linux ++ (with pkgs; [
    # cargo fetches every git dependency through the git CLI. On macOS this
    # real git shadows the /usr/bin/git xcrun shim.
    git
    openssl.dev
    # `just test`.
    cargo-nextest
    # Justfile helpers (`_architect-ui-link`).
    python3
  ])
  ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
    alsa-lib.dev
    pipewire.dev
    # Sets bindgen's clang arguments for crates that run it (pipewire-sys).
    rustPlatform.bindgenHook
  ]);

  env = {
    OPENSSL_DIR = "${pkgs.openssl.dev}";
    OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
  }
  // lib.optionalAttrs pkgs.stdenv.isLinux {
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
    # The loader finds what the linker found (and what wgpu dlopens), so a
    # test binary or a standalone plugin runs from this shell.
    LD_LIBRARY_PATH = lib.makeLibraryPath (common ++ linux ++ [ pkgs.stdenv.cc.cc.lib ]);
  };

  # kache is not set here: the rust module exports RUSTC_WRAPPER from PATH.

  fleet.banner = ''
    plugins dev shell
      just check / just test / just wasm-check
      just plugins-bundle
  '';
}
