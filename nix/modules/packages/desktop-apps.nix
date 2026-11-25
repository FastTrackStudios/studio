# The desktop programs, built as ordinary crane packages —
# `nix run .#studio-desktop` (Studio, apps/desktop: the `studio` binary),
# `nix run .#engine-host` (Session's host, features/engine/host). Not
# the notarized/bundled release artifacts (scripts/desktop/*); a plain
# native binary for local use (cargoArtifacts = null — the workspace is too
# large/build.rs-heavy for crane's dummy-src deps split).
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }:
    let
      # One version for the whole studio: the workspace's. The crates take it
      # with `version.workspace = true`, so the packages cannot drift from it.
      workspaceVersion = (builtins.fromTOML (builtins.readFile ../../../Cargo.toml)).workspace.package.version;

      # Reuse the toolchain's own GUI dep list (toolchain.nix already
      # assembled webkitgtk/gtk3/x11/vulkan for Linux, libiconv for
      # Darwin) instead of duplicating it — commonArgs.buildInputs alone
      # only carries openssl, which is enough for headless crates
      # (task-server) but not a wry/tao desktop window.
      guiArgs = {
        buildInputs = config.fts.buildInputs;
        # python3 explicitly: stylo's build.rs (nice-plug-dioxus/Blitz, a
        # transitive dep of the desktop GUI) shells out to `python3` and
        # needs it on PATH, not just linkable (same fix as fts-plugins in
        # reaper.nix).
        nativeBuildInputs = config.fts.nativeBuildInputs ++ [ pkgs.python3 ];
      };

      engine-host = config.fts.craneLib.buildPackage (config.fts.commonArgs // guiArgs // {
        pname = "engine-host";
        version = workspaceVersion;
        cargoArtifacts = null;
        cargoExtraArgs = "--package engine-host";
        doCheck = false;
        meta.mainProgram = "engine-host";
      });

      # Studio: the Architect distribution (apps/desktop), its `studio` binary.
      studio-desktop = config.fts.craneLib.buildPackage (config.fts.commonArgs // guiArgs // {
        pname = "studio-desktop";
        version = workspaceVersion;
        cargoArtifacts = null;
        cargoExtraArgs = "--package studio-desktop --bin studio";
        doCheck = false;
        meta.mainProgram = "studio";
      });
    in
    {
      packages = { inherit engine-host studio-desktop; };
    };
}
