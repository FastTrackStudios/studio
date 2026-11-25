{
  description = "frame — nix develop shells (the dev shell and CI are devenv: devenv.nix) with the toolchain and everything the workspace links against";

  inputs = {
    # The libraries' nixpkgs, as before (5e11f7ac). Not the fleet's newer one:
    # on it, a GTK/WebKit shell's -isystem/-L/-rpath flags can pass the
    # 128 KiB Linux allows COLLECT_GCC_OPTIONS, and every link fails with
    # "Argument list too long" (measured on patchbay's flake). The dev shell
    # and CI are devenv (devenv.nix); this flake stays for `nix develop`.
    nixpkgs.url = "github:nixos/nixpkgs/5e11f7acce6c3469bef9df154d78534fa7ae8b6c";
    # The toolchain: devenv-fleet at the revision devenv.yaml imports,
    # evaluated on ITS pinned nixpkgs and rust-overlay, so rustc is the
    # devenv shell's store path. Move it with devenv.yaml.
    devenv-fleet.url = "github:FastTrackStudios/devenv-fleet/c4ddec850ad97b5da74c746bff39a988f02b13a4";
  };

  outputs = { self, nixpkgs, devenv-fleet }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system:
        f (import nixpkgs { inherit system; }));
    in
    {
      devShells = forAll (pkgs:
        let
          inherit (pkgs) lib stdenv;
          # The fleet toolchain: the expression devenv-fleet's `rust` module
          # uses, so the same rustc store path as `devenv shell`.
          toolchain = devenv-fleet.lib.toolchainFor (import devenv-fleet.inputs.nixpkgs {
            inherit (stdenv.hostPlatform) system;
            overlays = [ devenv-fleet.overlays.default ];
          });
          linux = with pkgs; lib.optionals stdenv.hostPlatform.isLinux [
            # The architect/Dioxus crates link the native GUI stack even in
            # unit tests.
            xdotool webkitgtk_4_1 gtk3 libsoup_3 glib cairo pango gdk-pixbuf atk harfbuzz dbus
            libx11 libxcb libxcursor libxrandr libxi
            # wgpu (the Vello renders, the parity suite) dlopens the Vulkan loader.
            vulkan-loader libGL wayland libxkbcommon
          ];
          common = with pkgs; [ openssl zlib fontconfig freetype stdenv.cc.cc.lib ];
          tools = with pkgs; [ toolchain pkg-config just cargo-nextest git python3 imagemagick ]
            ++ lib.optionals stdenv.hostPlatform.isLinux [ mold ];
        in
        rec {
          default = pkgs.mkShell {
            packages = tools ++ common ++ linux;
            # The loader needs the libraries the linker found, plus the host
            # GPU driver on NixOS (/run/opengl-driver) for wgpu.
            LD_LIBRARY_PATH = lib.makeLibraryPath (common ++ linux)
              + lib.optionalString stdenv.hostPlatform.isLinux ":/run/opengl-driver/lib";
          };
          ci = default;
        });
    };
}
