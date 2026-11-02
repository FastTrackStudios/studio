{
  description = "frame — dev and CI shells with the toolchain and everything the workspace links against";

  inputs = {
    # The same nixpkgs and rust-overlay Studio locks, so the fleet's
    # caches already hold the shell.
    nixpkgs.url = "github:nixos/nixpkgs/5e11f7acce6c3469bef9df154d78534fa7ae8b6c";
    rust-overlay.url = "github:oxalica/rust-overlay/6ecf534a7c16e7c969f26838fdd8c93597ae93f3";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system:
        f (import nixpkgs { inherit system; overlays = [ rust-overlay.overlays.default ]; }));
    in
    {
      devShells = forAll (pkgs:
        let
          inherit (pkgs) lib stdenv;
          # The fleet's pin (Studio's nix/modules/toolchain.nix).
          toolchain = pkgs.rust-bin.stable."1.99.1".default.override {
            extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
          };
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
