# cargo-rail — CI change selection (`cargo rail plan`): which workspace
# packages a change actually affects, through cargo's resolved
# reverse-dependency graph. studio is a big workspace, so running only
# the affected tests is worth real minutes per promotion PR. nixpkgs
# carries 0.7.0, which predates the plan contract, so it is built here
# from the release tag (same version and hashes as Task's module).
#
# Built with its OWN 1.98.1 toolchain, not fts.rustToolchain: rail's
# rust-version is 1.98.1 and the workspace pin is 1.94, whose
# rust-overlay (followed from the dioxus-flake hub) has nothing newer
# than 1.94.1. The `rust-overlay-rail` input (flake.nix) supplies the
# compiler for this one package; the workspace toolchain is untouched.
# rail only reads the workspace (cargo metadata, git), so the compiler
# that built it does not matter to its answers.
#
# The resolve-graph patch this module used to carry for 0.17.3 is gone:
# upstream keys graph nodes by exact PackageId and builds edges from
# cargo's resolved graph since the 0.2x line, which is what the patch
# did (crates.io's iroh dev-depending on a crate named `patchbay` no
# longer aliases ours).
#
# Only `plan` (and `config`) are used. rail's compiler cache (`cache
# setup` installs a global cargo wrapper) stays off: the runners keep a
# warm target dir. `release` is inert in .config/rail.toml; releases go
# through scripts/release.sh.
{ inputs, ... }:
{
  perSystem = { pkgs, lib, ... }:
    let
      railRust = (pkgs.extend inputs.rust-overlay-rail.overlays.default)
        .rust-bin.stable."1.98.1".default;
      rustPlatform = pkgs.makeRustPlatform {
        cargo = railRust;
        rustc = railRust;
      };
      cargo-rail = rustPlatform.buildRustPackage rec {
        pname = "cargo-rail";
        version = "0.32.0";
        src = pkgs.fetchFromGitHub {
          owner = "loadingalias";
          repo = "cargo-rail";
          rev = "v${version}";
          hash = "sha256-XdwwQVv3OIauYefmQNq6CKK/fcaEnmq6Rkilmg9bhk8=";
        };
        cargoHash = "sha256-kpFJ3/s1qgXqY7Nhci7/4YtMcw7E+UmB3SX8F5MHoxs=";
        doCheck = false;
        meta = {
          description = "Cargo-native monorepo change selection (plan), unify, release";
          homepage = "https://github.com/loadingalias/cargo-rail";
          license = lib.licenses.mit;
          mainProgram = "cargo-rail";
        };
      };
    in
    {
      fts.cargoRail = cargo-rail;
    };
}
