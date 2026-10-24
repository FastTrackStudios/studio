//! Steps more than one job uses, and the pinned tool versions the
//! GitHub-hosted jobs install.
//!
//! The hosted jobs run WITHOUT nix. Until October 2026 every hosted job
//! installed nix and realised the `.#ci` dev shell from cachix (~1.5 GiB to
//! download, 7.7 GiB unpacked) just to get a toolchain and a handful of
//! tools; now each tool comes from its own pinned release:
//!
//! - rustc: `dtolnay/rust-toolchain` at [`RUST`], the workspace pin
//!   (nix/modules/toolchain.nix, `rust-bin.stable."1.94.0"`), with only the
//!   components and targets a job needs;
//! - the native headers the ci shell provided (nix/modules/toolchain.nix
//!   `fts.buildInputs` on Linux) as Ubuntu packages, [`APT_PACKAGES`],
//!   cached by `awalsh128/cache-apt-pkgs-action` the way Dioxus does;
//! - just from `taiki-e/install-action`;
//! - cargo-rail and actionlint as upstream release
//!   binaries, checked against a pinned sha256 ([`Pinned`]) — the same
//!   artefacts nix fetched;
//! - python3 from the runner image (/usr/bin/python3).
//!
//! The SELF-HOSTED jobs (CI's Check, Tests and Wasm on THEBATTLESHIP's studio
//! pools; the release builds on `studio-release`) keep nix: there it is the host's own package manager, not
//! an install step (their commands run in the repo's `.#ci` dev shell).

use crate::model::Step;

/// The workspace toolchain pin. Keep in step with nix/modules/toolchain.nix.
pub const RUST: &str = "1.94.0";

/// `just`, the version the nix ci shell carries (nixpkgs).
const JUST: &str = "1.48.1";

/// A release binary pinned by version and the sha256 of its download.
pub struct Pinned {
    pub version: &'static str,
    pub sha256: &'static str,
}

/// cargo-rail: CI change selection (`cargo rail plan`) and release intent
/// (`cargo rail change check`). nix/modules/cargo-rail.nix builds 0.32.0 from
/// source with its own 1.98.1 toolchain (rail's MSRV is above the workspace
/// pin); upstream publishes a prebuilt x86_64-linux-gnu zip for every release,
/// so the hosted jobs take that instead (the sha256 is the release's
/// SHA256SUMS line). The zip holds rail's helper binaries beside the main one;
/// the whole directory goes on PATH.
const CARGO_RAIL: Pinned = Pinned {
    version: "0.32.0",
    sha256: "5fcda1a62d36d51834ad49a5ef2ebc82777fd9d134d3d409f7021ce1a495dee4",
};

/// actionlint lints the generated workflows in the lint job (and runs the
/// runner image's shellcheck over every `run:` script).
const ACTIONLINT: Pinned = Pinned {
    version: "1.7.12",
    sha256: "8aca8db96f1b94770f1b0d72b6dddcb1ebb8123cb3712530b08cc387b349a3d8",
};

/// The Ubuntu packages for the native headers nix/modules/toolchain.nix puts
/// in the ci shell on Linux (`fts.buildInputs` + the tools in
/// `fts.nativeBuildInputs`), so `cargo check --workspace` can run every build
/// script. Grouped as there:
///
/// - pkg-config, openssl, fontconfig, freetype (cmake and python3 are on the
///   image already);
/// - bindgen's libclang (`rustPlatform.bindgenHook`), and clang + llvm-ar
///   for the wasm32 C builds (ring) — `CC_wasm32_unknown_unknown` /
///   `AR_wasm32_unknown_unknown` in the shell;
/// - mold: .cargo/config.toml links every x86_64-linux build with
///   `-fuse-ld=mold`, so without it nothing links;
/// - alsa, udev (hidapi's build script pkg-configs it), JACK (cpal `jack`),
///   PipeWire (cpal `pipewire`; bindgen needs the libspa headers too);
/// - the GTK / WebKitGTK stack (glib gtk3 gdk-pixbuf pango cairo atk
///   harfbuzz libsoup_3 webkitgtk_4_1 xdotool);
/// - X11 / xkbcommon / wayland / GL / Vulkan.
///
/// onnxruntime is not here: `ort` is load-dynamic and only needs the .so at
/// run time, and no hosted job runs anything.
const APT_PACKAGES: &[&str] = &[
    "pkg-config",
    "libssl-dev",
    "libfontconfig1-dev",
    "libfreetype-dev",
    "libclang-dev",
    "clang",
    "llvm",
    "mold",
    "libasound2-dev",
    "libudev-dev",
    "libjack-jackd2-dev",
    "libpipewire-0.3-dev",
    "libspa-0.2-dev",
    "libglib2.0-dev",
    "libgtk-3-dev",
    "libharfbuzz-dev",
    "libsoup-3.0-dev",
    "libwebkit2gtk-4.1-dev",
    "libxdo-dev",
    "libx11-dev",
    "libxcursor-dev",
    "libxrandr-dev",
    "libxi-dev",
    "libxcb1-dev",
    "libxkbcommon-dev",
    "libwayland-dev",
    "libgl-dev",
    "libvulkan-dev",
];

/// lavapipe, Mesa's software Vulkan driver, for the jobs that RUN tests
/// without a GPU (the wgpu / Vello tests need a Vulkan ICD).
const GPU_APT_PACKAGES: &[&str] = &["mesa-vulkan-drivers"];

/// cargo-nextest, the version the nix ci shell carries (nixpkgs), for a job
/// that runs the tests without nix.
const NEXTEST: &str = "0.9.132";

/// Bump to throw away the apt cache (cache-apt-pkgs-action keys on the
/// package list and this).
const APT_CACHE_VERSION: &str = "1";

/// The event's own ref: a PR's merge ref, the pushed commit, main for the
/// nightly, a dispatched branch.
pub fn checkout() -> Step {
    Step::action("actions/checkout@v4")
}

/// Full history (`fetch-depth: 0`), at an optional ref.
pub fn checkout_full(git_ref: Option<&str>) -> Step {
    let step = checkout();
    let step = match git_ref {
        Some(r) => step.with("ref", r),
        None => step,
    };
    step.with("fetch-depth", 0)
}

/// The release version from the tag, into this checkout (scripts/release.sh
/// `stamp`): the FIRST step after the checkout of every job that builds or
/// judges a release. Releases commit no version; the tag is the version, so
/// `env!("CARGO_PKG_VERSION")`, the iOS marketing version, the desktop app
/// version and the update check see it only once it is written into the
/// workspace version and Cargo.lock's member entries (uncommitted). `tag` is
/// a workflow expression (`${{ env.TAG }}`, `${{ github.ref_name }}`).
///
/// A tag cut before the stamp existed (a re-run of v26.10.3 or older) carries
/// its version in its commit, and its script has no `stamp`: then the step
/// only says so, and the tag guard checks the committed version as before.
pub fn stamp(tag: &str) -> Step {
    Step::run(
        "Stamp the release version from the tag",
        r#"set -euo pipefail
if grep -q '^    stamp)' scripts/release.sh; then
  bash scripts/release.sh stamp "$STAMP_TAG"
else
  echo "$STAMP_TAG predates the stamp: its commit carries its version ($(bash scripts/release.sh version))"
fi
"#,
    )
    .env("STAMP_TAG", tag)
}

/// The tools a hosted job installs on top of the toolchain.
#[derive(Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools)] // a set of independent switches
pub struct Tools {
    pub apt: bool,
    pub just: bool,
    pub cargo_rail: bool,
    pub actionlint: bool,
    pub nextest: bool,
    /// lavapipe (software Vulkan) for running the GPU tests.
    pub gpu: bool,
    /// Copy .cargo/ci-config.toml into ../.cargo/config.toml.
    pub ci_cargo_config: bool,
    /// Free the image's disk (a job that builds).
    pub free_disk: bool,
    /// mold alone, for a job without the full [`APT_PACKAGES`] list:
    /// .cargo/config.toml links every x86_64-linux build with it, so even
    /// `cargo run -p xtask-workflows` needs it.
    pub mold: bool,
}

/// Everything a hosted job needs before its first cargo command.
pub fn hosted_setup(components: &str, targets: &str, tools: Tools) -> Vec<Step> {
    let mut steps = Vec::new();
    if tools.free_disk {
        steps.push(free_disk());
    }
    steps.push(rust_toolchain(components, targets));
    if tools.apt {
        steps.push(apt_packages(tools.gpu));
    } else if tools.mold {
        steps.push(Step::run(
            "mold",
            "sudo apt-get install -y --no-install-recommends mold\n",
        ));
    }
    if tools.just {
        steps.push(
            Step::uses("just", "taiki-e/install-action@v2").with("tool", format!("just@{JUST}")),
        );
    }
    if tools.nextest {
        steps.push(
            Step::uses("cargo-nextest", "taiki-e/install-action@v2")
                .with("tool", format!("cargo-nextest@{NEXTEST}")),
        );
    }
    if tools.cargo_rail {
        steps.push(install_cargo_rail());
    }
    if tools.actionlint {
        steps.push(install_actionlint());
    }
    if tools.ci_cargo_config {
        steps.push(ci_cargo_config());
    }
    steps
}

/// The image's preinstalled SDKs we never use (the toolchain comes from
/// rustup into ~/.rustup, the headers from apt into /usr): ~25 GB back. In the
/// background, so nothing waits on it; it is done long before the target dir
/// grows. /opt/hostedtoolcache goes too — the python3 the scripts use is the
/// system /usr/bin/python3, not a toolcache one.
fn free_disk() -> Step {
    Step::run(
        "Free disk (background)",
        "\
df -h / | tail -n 1
nohup sudo rm -rf /usr/local/lib/android /usr/share/dotnet /opt/ghc \\
  /usr/local/.ghcup /opt/hostedtoolcache /usr/share/swift \\
  /usr/local/share/powershell /usr/local/share/chromium \\
  /usr/local/share/boost /opt/microsoft /opt/google \\
  </dev/null >/dev/null 2>&1 &
",
    )
}

/// The workspace pin, with only what the job needs. dtolnay's action makes it
/// the rustup default, so it shadows the image's preinstalled stable.
fn rust_toolchain(components: &str, targets: &str) -> Step {
    let step = Step::uses(
        &format!("Rust {RUST}"),
        &format!("dtolnay/rust-toolchain@{RUST}"),
    );
    let step = if components.is_empty() {
        step
    } else {
        step.with("components", components)
    };
    if targets.is_empty() {
        step
    } else {
        step.with("targets", targets)
    }
}

fn apt_packages(gpu: bool) -> Step {
    let extra: &[&str] = if gpu { GPU_APT_PACKAGES } else { &[] };
    Step::uses(
        "Native headers (apt, cached)",
        "awalsh128/cache-apt-pkgs-action@v1.6.3",
    )
    .with("packages", [APT_PACKAGES, extra].concat().join(" "))
    .with("version", APT_CACHE_VERSION)
}

/// Download `url` to `$RUNNER_TEMP/<file>` and check its sha256. Every
/// installer below puts its binary in ~/.local/bin, which it adds to PATH.
fn fetch(url: &str, file: &str, sha256: &str) -> String {
    format!(
        "\
set -euo pipefail
mkdir -p \"$HOME/.local/bin\"
echo \"$HOME/.local/bin\" >> \"$GITHUB_PATH\"
curl -fsSL --retry 3 -o \"$RUNNER_TEMP/{file}\" \"{url}\"
echo \"{sha256}  $RUNNER_TEMP/{file}\" | sha256sum -c -
"
    )
}

fn install_cargo_rail() -> Step {
    let Pinned { version, sha256 } = CARGO_RAIL;
    let url = format!(
        "https://github.com/loadingalias/cargo-rail/releases/download/v{version}/cargo-rail-x86_64-unknown-linux-gnu.zip"
    );
    let mut script = fetch(&url, "cargo-rail.zip", sha256);
    script.push_str(
        "\
unzip -q -o \"$RUNNER_TEMP/cargo-rail.zip\" -d \"$HOME/.local/share\"
echo \"$HOME/.local/share/cargo-rail\" >> \"$GITHUB_PATH\"
\"$HOME/.local/share/cargo-rail/cargo-rail\" --version
",
    );
    Step::run(&format!("cargo-rail {version}"), &script)
}

fn install_actionlint() -> Step {
    let Pinned { version, sha256 } = ACTIONLINT;
    let url = format!(
        "https://github.com/rhysd/actionlint/releases/download/v{version}/actionlint_{version}_linux_amd64.tar.gz"
    );
    let mut script = fetch(&url, "actionlint.tar.gz", sha256);
    script.push_str(
        "\
tar -xzf \"$RUNNER_TEMP/actionlint.tar.gz\" -C \"$HOME/.local/bin\" actionlint
\"$HOME/.local/bin/actionlint\" -version
",
    );
    Step::run(&format!("actionlint {version}"), &script)
}

/// CI-only cargo settings (.cargo/ci-config.toml, the Zed trick). Copied to
/// the PARENT of the checkout's .cargo, so cargo MERGES it with the repo's
/// .cargo/config.toml (the repo's wins on a conflict; arrays such as
/// rustflags concatenate) instead of an env var REPLACING every rustflag
/// (rust-lang/cargo#5376). It carries what the `CARGO_PROFILE_DEV_DEBUG`
/// env used to pin; see the file for the rest.
pub fn ci_cargo_config() -> Step {
    Step::run(
        "CI cargo config",
        "\
mkdir -p ../.cargo
cp .cargo/ci-config.toml ../.cargo/config.toml
",
    )
}

/// Swatinem/rust-cache with ONE shared key per build kind, saved by main
/// only: PRs restore main's entry and never write their own, so a cache
/// budget holds one set per kind instead of being evicted by every open PR. `cache-on-failure`: the cache holds compiled dependencies,
/// not results.
///
/// No `key:` — rust-cache already fingerprints the active rustc
/// (`rustc -vV`, i.e. the [`RUST`] pin) and the lockfiles/manifests. (It
/// used to be `hashFiles('flake.lock')`, because the toolchain was nix's and
/// not the rustc on the image's PATH that rust-cache fingerprints.)
///
/// Followed by Dioxus's prune-cargo-target (.github/actions): it marks every
/// fingerprint right after the restore and, at the end of a successful job,
/// deletes every unit the build did not read, so the entry main saves holds
/// only what this build used instead of piling up the `-<hash>` units of
/// every past Cargo.lock.
pub fn rust_cache(shared_key: &str) -> [Step; 2] {
    [
        Step::uses(
            &format!("Rust cache ({shared_key}, main saves)"),
            "Swatinem/rust-cache@v2",
        )
        .with("shared-key", shared_key)
        .with("save-if", "${{ github.ref == 'refs/heads/main' }}")
        .with("cache-on-failure", true)
        .with("cache-bin", false),
        Step::uses(
            "Prune unused units before the cache is saved",
            "./.github/actions/prune-cargo-target",
        )
        .with("target-dir", "target"),
    ]
}

/// What [`cargo_deps_restore`] / [`cargo_deps_save`] keep: cargo's
/// downloaded registry and git dependencies (not anything built).
const CARGO_DEPS_PATHS: &str = "\
~/.cargo/registry/index
~/.cargo/registry/cache
~/.cargo/git/db
";

/// Keyed on the lockfile, which content-addresses every download.
const CARGO_DEPS_KEY: &str = "cargo-deps-${{ runner.os }}-${{ hashFiles('Cargo.lock') }}";

/// cargo's downloaded dependencies from the Actions cache, for a free job
/// that runs cargo but builds nothing (the PR checks). Without it the first
/// cargo command spends most of a minute fetching every git dependency.
/// Only main writes the entry (CI's `deps-cache` job, [`cargo_deps_save`]),
/// so PRs never spend the 10 GB budget.
pub fn cargo_deps_restore() -> Step {
    Step::uses("Restore cargo's dependencies", "actions/cache/restore@v4")
        .id("deps")
        .with("path", CARGO_DEPS_PATHS)
        .with("key", CARGO_DEPS_KEY)
        .with("restore-keys", "cargo-deps-${{ runner.os }}-")
}

/// [`cargo_deps_restore`], then on a miss fetch everything and save it.
pub fn cargo_deps_save() -> [Step; 3] {
    let miss = "${{ steps.deps.outputs.cache-hit != 'true' }}";
    [
        cargo_deps_restore(),
        Step::run("Fetch cargo's dependencies", "cargo fetch").cond(miss),
        Step::uses("Save cargo's dependencies", "actions/cache/save@v4")
            .cond(miss)
            .with("path", CARGO_DEPS_PATHS)
            .with("key", CARGO_DEPS_KEY),
    ]
}

/// Link a self-hosted runner's rig-models clone into the tree, per
/// assets.txt: the guitar rig's release build refuses to compile without
/// its captures (features/rigs/guitar/build.rs), and they are in the private
/// rig-models repo, which the release runners keep one clone of beside their
/// other projects.
///
/// `fetch`: fast-forward the clone to origin/main first, so a capture pushed
/// to rig-models ships in the next build, and fail rather than build with
/// stale captures (https, so the fts-forge credential helper authenticates:
/// airlock has no GitHub ssh key). Without it (a runner that has no
/// credential for the private repo) the clone is linked as it is. Inline,
/// not a script in the tree: a re-run of an older tag builds that tag's
/// sources, which do not have one.
pub fn rig_models(src: &str, fetch: bool) -> String {
    let sync = if fetch {
        r#"url=https://github.com/FastTrackStudios/rig-models.git
if [ -d "$src/.git" ]; then
  before="$(git -C "$src" rev-parse --short HEAD)"
  git -C "$src" fetch --quiet origin \
    || { echo "::error::could not fetch rig-models into $src; not building with stale captures"; exit 1; }
  git -C "$src" merge --ff-only --quiet origin/main \
    || { echo "::error::$src cannot fast-forward to origin/main (local commits or changes?); not building with stale captures"; exit 1; }
  echo "rig-models $before -> $(git -C "$src" rev-parse --short HEAD)"
else
  git clone --quiet "$url" "$src" \
    || { echo "::error::could not clone $url into $src"; exit 1; }
  echo "rig-models cloned at $(git -C "$src" rev-parse --short HEAD)"
fi
"#
    } else {
        r#"[ -d "$src" ] || { echo "::error::no rig-models clone at $src (this runner does not fetch it)"; exit 1; }
"#
    };
    format!(
        r#"set -euo pipefail
src={src}
{sync}if [ ! -d "$src/guitar" ] || [ ! -d "$src/bass" ]; then echo "::error::no rig-models clone at $src"; exit 1; fi
while read -r repo from to; do
  [[ -z $repo || $repo == \#* || $repo != rig-models ]] && continue
  to="${{to:-$from}}"
  n="$(find -L "$src/$from" -type f \( -name '*.nam' -o -name '*.wav' \) | wc -l | tr -d ' ')"
  [ "$n" -gt 0 ] || {{ echo "::error::$src/$from holds no captures"; exit 1; }}
  mkdir -p "$(dirname "$to")"
  if [ -d "$to" ] && [ ! -L "$to" ]; then rmdir "$to"; fi
  ln -sfn "$src/$from" "$to"
  echo "linked $to -> $src/$from ($n captures)"
done < assets.txt
echo "rig-models at $(git -C "$src" rev-parse --short HEAD 2>/dev/null || echo unknown)"
"#
    )
}
