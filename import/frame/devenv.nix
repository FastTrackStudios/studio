# frame's dev environment. The toolchain (rust 1.99.1, the
# rust-toolchain.toml pin), mold, cargo-hakari, cargo-rail and prek come
# from devenv-fleet's `rust` module (devenv.yaml); this file adds what this
# workspace links against and runs. flake.nix's `ci` shell is what CI uses,
# so the two package lists must stay in step.
{ pkgs, lib, ... }:
let
  # The architect/Dioxus crates link the native GUI stack even in unit
  # tests, and wgpu (the Vello renders, the parity suite) dlopens the Vulkan
  # loader.
  linux = with pkgs; lib.optionals stdenv.hostPlatform.isLinux [
    xdotool webkitgtk_4_1 gtk3 libsoup_3 glib cairo pango gdk-pixbuf atk harfbuzz dbus
    libx11 libxcb libxcursor libxrandr libxi
    vulkan-loader libGL wayland libxkbcommon
  ];
  common = with pkgs; [ openssl zlib fontconfig freetype stdenv.cc.cc.lib ];
in
{
  packages = common ++ linux ++ (with pkgs; [
    pkg-config just cargo-nextest git python3 imagemagick
  ]);

  # The loader needs the libraries the linker found, plus the host GPU
  # driver on NixOS (/run/opengl-driver) for wgpu.
  env.LD_LIBRARY_PATH = lib.makeLibraryPath (common ++ linux)
    + lib.optionalString pkgs.stdenv.hostPlatform.isLinux ":/run/opengl-driver/lib";
}
