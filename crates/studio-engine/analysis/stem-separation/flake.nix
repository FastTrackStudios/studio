{
  description = "Stem separation inference environment";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/5e11f7acce6c3469bef9df154d78534fa7ae8b6c";

  outputs = { nixpkgs, ... }: {
    devShells = nixpkgs.lib.genAttrs [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
      "x86_64-darwin"
    ] (system:
      let pkgs = nixpkgs.legacyPackages.${system};
      in {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc uv python312 ffmpeg sox flac ];
          shellHook = pkgs.lib.optionalString pkgs.stdenv.isLinux ''
            export LD_LIBRARY_PATH="/run/opengl-driver/lib:${pkgs.lib.makeLibraryPath [
              pkgs.stdenv.cc.cc.lib
              pkgs.zlib
            ]}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
          '';
        };
      });
  };
}
