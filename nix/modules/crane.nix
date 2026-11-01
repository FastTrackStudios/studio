# crane plumbing shared by every deployable package (fills fts.craneLib,
# fts.src, fts.commonArgs, fts.buildRev, fts.buildTag, fts.buildDate).
{ inputs, self, ... }:
{
  perSystem = { pkgs, lib, config, ... }:
    let
      # The whole monorepo is the build source: ONE workspace, so the
      # root Cargo.lock drives vendoring, and the [patch.crates-io]
      # path patches (libs/vendor/styx-format, libs/editor/vendor/
      # mermaid-rs-renderer) resolve in-tree. Keep the filter minimal
      # — the flake source is already the tracked git tree (no
      # target/, no untracked junk); we only strip obvious non-build
      # dirs to cut store-copy churn.
      ftsSrc = lib.cleanSourceWith {
        src = ../..;
        filter = path: type:
          let name = builtins.baseNameOf (toString path); in
          !(builtins.elem name [ "target" "node_modules" ".git" "result" "web-dist" ]);
      };

      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain config.fts.rustToolchain;

      # reaper-low (git dep on github.com/FastTrackStudios/reaper-rs)
      # mounts justinfrankel/WDL as a git submodule at main/low/lib/WDL.
      # crane's git-dep vendoring lists files via `cargo package -l`,
      # which walks git-TRACKED files from the superproject's index —
      # a submodule's content isn't in that index (only its gitlink is),
      # so the vendored reaper-low ends up with an empty/partial lib/WDL
      # and its build.rs's own live `git submodule update` then fails
      # silently in the network-off nix sandbox ("no such file or
      # directory: lib/WDL/WDL/projectcontext.cpp"). Fetch the exact
      # submodule commit (pinned via reaper-rs's .gitmodules) and inject
      # it post-vendor instead.
      wdlSrc = pkgs.fetchFromGitHub {
        owner = "justinfrankel";
        repo = "WDL";
        rev = "40df69c0b9124ac433347f31c3f467054a8bfde8";
        hash = "sha256-sOVDXTNydgpetvC1D+DBF1XmR6k6KZbQ9mXKPOBPuTg=";
      };

      # Vendor against the root Cargo.lock. (The old baseview fetch
      # override is gone: the dead Codys-Wright/baseview.git dep was
      # vendored as a patch 2026-07-16; now from FastTrackStudios/vendor.)
      cargoVendorDir = craneLib.vendorCargoDeps {
        src = ftsSrc;

        # libspa-sys needs bindgen's `clang_macro_fallback()` to evaluate
        # cast-expression C macros its normal cexpr parser can't fold —
        # notably SPA_ID_INVALID (`((uint32_t)0xffffffff)`). That fallback
        # writes a scratch `.macro_eval.c` + `*-precompile.h.pch` into
        # `clang_macro_fallback_build_dir`, which DEFAULTS TO THE PROCESS
        # CWD — and cargo runs a build script with CWD set to the crate's
        # own source dir. Under nix that dir is the vendored copy in the
        # read-only store (dr-xr-xr-x), so the write fails; bindgen then
        # swallows it (`tu.save(&pch).ok()?` in ir/context.rs) and
        # SILENTLY disables macro fallback, emitting bindings with
        # SPA_ID_INVALID simply absent. The failure only surfaces much
        # later, as `libspa` failing to compile with a baffling
        # "cannot find value SPA_ID_INVALID in crate spa_sys".
        #
        # Ordinary `cargo build` never hits this: ~/.cargo/registry/src
        # is user-writable, so the scratch files land next to the crate
        # and everything works — which is why this reproduced ONLY inside
        # nix and looked, misleadingly, like a feature-unification bug.
        #
        # Point the fallback at OUT_DIR (where scratch build artifacts
        # belong) instead. Upstream should arguably do this itself.
        #
        # Both pipewire-rs `-sys` crates have it (libspa-sys drops
        # SPA_ID_INVALID, pipewire-sys drops PW_ID_ANY). The injected
        # expression is deliberately self-contained rather than reusing
        # each crate's `out_path` local: in pipewire-sys the
        # `.clang_macro_fallback()` call site comes *before* `out_path`
        # is even declared, so referencing it wouldn't compile.
        overrideVendorCargoPackage = p: drv:
          if p.name == "libspa-sys" || p.name == "pipewire-sys" then
            drv.overrideAttrs (old: {
              postInstall = (old.postInstall or "") + ''
                substituteInPlace $out/build.rs \
                  --replace-fail \
                    '.clang_macro_fallback()' \
                    '.clang_macro_fallback().clang_macro_fallback_build_dir(::std::path::PathBuf::from(::std::env::var("OUT_DIR").unwrap()))'
              '';
            })
          else
            drv;

        # Every git checkout: drop the `Cargo.toml.orig` files a repo of
        # vendored crates carries (FastTrackStudios/vendor keeps the
        # crates.io tarballs' as they came). crane lists each crate with
        # `cargo package -l`, which refuses a source holding that reserved
        # name ("invalid inclusion of reserved file name Cargo.toml.orig"),
        # and the whole vendor step, so every crane build, failed with it.
        # cargo writes its own when it packages; nothing reads these.
        overrideVendorGitCheckout = ps: drv:
          let
            noOrig = drv.overrideAttrs (old: {
              preInstall = (old.preInstall or "") + ''
                find . -name Cargo.toml.orig -type f -delete
              '';
            });
          in
          if lib.any (p: p.name == "reaper-low") ps then
            noOrig.overrideAttrs (old: {
              postInstall = (old.postInstall or "") + ''
                for d in $out/reaper-low-*; do
                  rm -rf "$d/lib/WDL"
                  cp -r ${wdlSrc} "$d/lib/WDL"
                  chmod -R u+w "$d/lib/WDL"
                done
              '';
            })
          else
            noOrig;
      };
      # The dummy source every `buildDepsOnly` builds from, the same for
      # every release. Release builds used to stamp the tag's version into
      # their checkout (`[workspace.package] version` and the workspace
      # members' Cargo.lock entries), and crane's own dummy source keeps
      # both, so each release had new dependency derivations and compiled
      # every dependency again. The stamp no longer touches either file
      # (scripts/release.sh stamp: the version reaches the build as
      # FTS_RELEASE_VERSION); this stays so a bump of the committed
      # version does not rebuild the dependencies either. Here the
      # workspace version reads 0.0.0 in both files, in the source and its
      # inputs alike (the Cargo.toml and the lock are rewritten at
      # evaluation, laundered of the flake source's context, as crane's
      # mkDummySrc does): only a change to a dependency, a manifest's
      # dependency tables or a .cargo config makes a new one. The members
      # are stubs here, so their version matters to nothing compiled; the
      # package built on top (`cargoArtifacts`) uses the real, stamped
      # source.
      depsDummySrc =
        let
          normal = "0.0.0";
          ws = (builtins.fromTOML (builtins.readFile ../../Cargo.toml)).workspace.package.version;
          lockText = builtins.unsafeDiscardStringContext (builtins.readFile ../../Cargo.lock);
          # What the stamp rewrites: every package without a source at the
          # workspace version, and the `"<name> <version>"` references to it.
          members = map (p: p.name)
            (builtins.filter (p: !(p ? source) && p.version == ws) (builtins.fromTOML lockText).package);
          refsFrom = map (n: ''"${n} ${ws}"'') members;
          refsTo = map (n: ''"${n} ${normal}"'') members;
          normaliseBlock = block:
            let
              isMember = lib.hasPrefix "[[package]]\n" block && !(lib.hasInfix "\nsource = " block);
              own =
                if isMember then
                  lib.concatMapStringsSep "\n"
                    (l: if l == ''version = "${ws}"'' then ''version = "${normal}"'' else l)
                    (lib.splitString "\n" block)
                else block;
            in
            if lib.hasInfix " ${ws}\"" own then builtins.replaceStrings refsFrom refsTo own else own;
          normalLock = lib.concatMapStringsSep "\n\n" normaliseBlock (lib.splitString "\n\n" lockText);
          root = toString ../..;
        in
        craneLib.mkDummySrc {
          # The real lock stays out: the normalised one is copied in instead.
          src = lib.cleanSourceWith {
            src = ftsSrc;
            filter = path: _type: !(baseNameOf path == "Cargo.lock" && dirOf path == root);
          };
          cargoLock = pkgs.writeText "Cargo.lock" normalLock;
          # [workspace.package] is only in the root manifest; its version is
          # dropped at evaluation and put back as 0.0.0.
          cleanCargoTomlFilter = p:
            craneLib.filters.cargoTomlDefault p && p != [ "workspace" "package" "version" ];
          extraDummyScript = ''
            sed -i '/^\[workspace\.package\]$/a version = "${normal}"' $out/Cargo.toml
            grep -qx 'version = "${normal}"' $out/Cargo.toml \
              || { echo "depsDummySrc: no [workspace.package] table in the dummy Cargo.toml" >&2; exit 1; }
          '';
        };
    in
    {
      fts.craneLib = craneLib;
      fts.src = ftsSrc;
      fts.depsDummySrc = depsDummySrc;

      fts.commonArgs = {
        src = ftsSrc;
        inherit cargoVendorDir;
        strictDeps = true;
        # mold: the repo's `.cargo/config.toml` pins
        # `-C link-arg=-fuse-ld=mold` for x86_64-linux (see the comment
        # there). That flag applies to nix builds too, and the sandbox has
        # no devshell — without mold on PATH, gcc's collect2 dies with
        # "cannot find 'ld'" on the very first build script. Ship the
        # linker the flag asks for rather than fighting the flag.
        nativeBuildInputs = with pkgs; [ pkg-config ]
          ++ lib.optional stdenv.isLinux mold;
        buildInputs = with pkgs; [ openssl ];
      };

      # kache inside the sandbox, opt-in: a build evaluated with
      # FTS_NIX_KACHE set (under --impure; pure evaluation reads getEnv as
      # "", so every other build is unchanged, hashes and all) wraps rustc
      # in kache over THEBATTLESHIP's shared store, so the release image
      # restores the crates the previous release compiled instead of
      # compiling them all again. Its outputs are impure in the sense that
      # matters: they depend on the store's contents, so nothing built this
      # way may be pushed to a binary cache.
      #
      # The host binds the store and a target-dir scratch into the sandbox,
      # both writable by group nixbld (nix-config
      # hosts/THEBATTLESHIP/kache-nix-sandbox.nix). Without them (another
      # host, the drive missing) the build goes on without kache.
      #
      # Only the derivations that compile everything themselves take these
      # (web-bundles.nix: session-app-web and the dx bundles); the
      # deps-only builds keep their pure hashes, so the copies already in
      # the nix store still serve.
      fts.kacheArgs =
        let
          store = "/run/media/Development/.kache";
          builds = "/run/media/Development/.kache-nix-builds";
          kache = inputs.kache.packages.${pkgs.stdenv.hostPlatform.system}.kache.overrideAttrs {
            # As nix-config's: upstream's CI runs the ~2000 tests.
            doCheck = false;
          };
          # The host's /etc/kache/config.toml is not in the sandbox: its
          # settings for a shared store, plus no GC from here (the host's
          # own daemon keeps the budget) and no remote.
          config = pkgs.writeText "kache-nix-sandbox.toml" ''
            [cache]
            local_store = "${store}"
            local_max_size = "930GiB"
            local_only = true
            auto_gc = false
            seed_new_targets = false
            auto_clean_orphaned_targets = false
            auto_share_target_files = false
            auto_clean_idle_targets_days = 0
            auto_clean_unused_units_days = 0
            clean_incremental = false
            daemon_idle_timeout_secs = 30
          '';
        in
        lib.optionalAttrs (builtins.getEnv "FTS_NIX_KACHE" != "") {
          RUSTC_WRAPPER = "${kache}/bin/kache";
          KACHE_CONFIG = "${config}";
          KACHE_HOST_CONFIG = "";
          # dx wraps every workspace crate in its own rustc wrapper (args
          # captured for hot-patching), which kache passes through uncached;
          # our dx skips it for a plain build with this set (dx.nix).
          DX_NO_RUSTC_WRAPPER = "1";
          preBuild = ''
            if [ -w ${store} ] && [ -w ${store}/index.db ]; then
              # Per build: kache's runtime dir (its daemon socket, logs) and
              # the OUT_DIR alias root, which must belong to the building
              # user. Both under $TMPDIR, /build in every build, so the
              # alias path compiled into a few proc macros is the same each
              # time.
              export KACHE_RUNTIME_DIR="$TMPDIR/kache"
              export KACHE_OUT_DIR_ALIAS_ROOT="$TMPDIR/kache-out"
              mkdir -p "$KACHE_RUNTIME_DIR"
              install -d -m 0700 "$KACHE_OUT_DIR_ALIAS_ROOT"
              # The target dir on the store's filesystem, so a restore is a
              # reflink; $TMPDIR is on /nix, where it would be a copy.
              # CARGO_TARGET_DIR, because kache keys it as <TARGET> (cargo
              # resolves a `target` symlink, so the per-build dir name
              # would reach the keys); `target` stays, as a link to it, for
              # the install phases. Killed builds' dirs are swept after a
              # day.
              if [ -w ${builds} ] && [ ! -e target ]; then
                find ${builds} -mindepth 1 -maxdepth 1 -mmin +1440 -exec rm -rf {} + 2>/dev/null || true
                ftsKacheTarget="$(mktemp -d ${builds}/"$pname".XXXXXX)"
                exitHooks+=('rm -rf "$ftsKacheTarget"')
                failureHooks+=('rm -rf "$ftsKacheTarget"')
                export CARGO_TARGET_DIR="$ftsKacheTarget/target"
                mkdir -p "$CARGO_TARGET_DIR"
                ln -s "$CARGO_TARGET_DIR" target
                # nixpkgs' cc/ld wrappers refuse to link a path outside the
                # nix store and the build dir ("impure path … used in
                # link"), and every build script links from here. In the
                # sandbox nothing else is outside them to guard against.
                export NIX_ENFORCE_PURITY=0
              fi
              echo "kache: on, store ${store}, target dir $(readlink -f target 2>/dev/null || echo target)"
            else
              echo "kache: ${store} is not writable in the sandbox; building without it"
              unset RUSTC_WRAPPER
            fi
          '';
          # The build's hits and misses, in its log.
          postBuild = ''
            if [ -n "''${RUSTC_WRAPPER:-}" ]; then
              "$RUSTC_WRAPPER" stats --last-build 2>&1 | head -n 24 || true
            fi
          '';
        };

      # Git rev baked into the deployable images so a running
      # deployment can say WHICH commit it serves (version.json /
      # TASK_BUILD_REV). Only the cheap wrapper layers depend on it —
      # the expensive cargo/wasm derivations stay rev-free and cached.
      #
      # A release job passes the tagged commit as FTS_BUILD_REV with
      # `--impure` (its stamp writes only the gitignored .release-version,
      # so the tree stays clean and self.rev is there too; the env wins).
      # Pure evaluation reads getEnv as "", so every other build is
      # unchanged.
      fts.buildRev =
        let fromEnv = builtins.getEnv "FTS_BUILD_REV";
        in if fromEnv != "" then fromEnv else self.rev or self.dirtyRev or "unknown";
      # The release the commit belongs to, as its tag: `v<FTS_RELEASE_VERSION>`
      # in a release job (scripts/release.sh stamp exports it; the image
      # build is `--impure`), else `v<committed workspace version>`. Only the
      # cheap wrapper layers read it (version.json, the image labels): no
      # cargo derivation changes with the release. And when the commit was
      # made (the flake's own lastModifiedDate, UTC): no clock in the build.
      fts.buildTag =
        let fromEnv = builtins.getEnv "FTS_RELEASE_VERSION";
        in "v" + (if fromEnv != "" then lib.removePrefix "v" fromEnv
                  else (builtins.fromTOML (builtins.readFile (self + "/Cargo.toml"))).workspace.package.version);
      fts.buildDate =
        let
          d = self.lastModifiedDate or "19700101000000";
          at = start: len: lib.substring start len d;
        in
        "${at 0 4}-${at 4 2}-${at 6 2}T${at 8 2}:${at 10 2}:${at 12 2}Z";
    };
}
