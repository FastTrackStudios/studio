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
//! - cargo-rail, tracey, tailwindcss and actionlint as upstream release
//!   binaries, checked against a pinned sha256 ([`Pinned`]) — the same
//!   artefacts (and for tracey the same hash) nix fetched;
//! - python3 from the runner image (/usr/bin/python3).
//!
//! The SELF-HOSTED jobs (`tests` on the NixOS VM, `deploy`) keep nix: there it
//! is the host's own package manager, not an install step ([`Toolchain::Nix`]).

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

/// tracey: the spec-coverage gate (`just daw-flows`). Same release tarball
/// nix/modules/tracey.nix fetches (its SRI hash is this sha256): NEVER
/// `cargo install`, whose build.rs needs node + pnpm and stalled CI for hours.
const TRACEY: Pinned = Pinned {
    version: "1.3.0",
    sha256: "fbc0c25c44328ccb0970b2509095ff29412fa72cbbc6b0036c4cee8c12821f47",
};

/// The tailwindcss standalone CLI, the version of the ci shell's
/// `tailwindcss_4` (nixpkgs). `just web-tailwind` needs it.
const TAILWIND: Pinned = Pinned {
    version: "4.2.2",
    sha256: "4ab84f2b496c402d3ec4fd25e0e5559fe1184d886dadae8fb4438344ec044c22",
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

/// Where a job's commands come from.
#[derive(Clone, Copy)]
pub enum Toolchain {
    /// A hosted job: the tools are on PATH (installed by [`hosted_setup`]).
    /// (The self-hosted NixOS variant went with the Mac Pro tests job.)
    Hosted,
}

impl Toolchain {
    /// `cmd` as this job runs it.
    pub fn cmd(self, cmd: &str) -> String {
        match self {
            Self::Hosted => cmd.to_owned(),
        }
    }
}

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

/// The tools a hosted job installs on top of the toolchain.
#[derive(Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools)] // a set of independent switches
pub struct Tools {
    pub apt: bool,
    pub just: bool,
    pub cargo_rail: bool,
    pub tracey: bool,
    pub tailwind: bool,
    pub actionlint: bool,
    pub nextest: bool,
    /// lavapipe (software Vulkan) for running the GPU tests.
    pub gpu: bool,
    /// Copy .cargo/ci-config.toml into ../.cargo/config.toml.
    pub ci_cargo_config: bool,
    /// Free the image's disk (a job that builds).
    pub free_disk: bool,
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
    if tools.tracey {
        steps.push(install_tracey());
    }
    if tools.tailwind {
        steps.push(install_tailwind());
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

fn install_tracey() -> Step {
    let Pinned { version, sha256 } = TRACEY;
    let url = format!(
        "https://github.com/bearcove/tracey/releases/download/v{version}/tracey-x86_64-unknown-linux-gnu.tar.xz"
    );
    let mut script = fetch(&url, "tracey.tar.xz", sha256);
    script.push_str(
        "\
tar -xJf \"$RUNNER_TEMP/tracey.tar.xz\" -C \"$RUNNER_TEMP\"
install -m755 \"$RUNNER_TEMP/tracey-x86_64-unknown-linux-gnu/tracey\" \"$HOME/.local/bin/tracey\"
\"$HOME/.local/bin/tracey\" --version
",
    );
    Step::run(&format!("tracey {version}"), &script)
}

fn install_tailwind() -> Step {
    let Pinned { version, sha256 } = TAILWIND;
    let url = format!(
        "https://github.com/tailwindlabs/tailwindcss/releases/download/v{version}/tailwindcss-linux-x64"
    );
    let mut script = fetch(&url, "tailwindcss", sha256);
    script
        .push_str("install -m755 \"$RUNNER_TEMP/tailwindcss\" \"$HOME/.local/bin/tailwindcss\"\n");
    Step::run(&format!("tailwindcss {version}"), &script)
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
/// only: PRs restore main's entry and never write their own, so the repo's
/// 10 GB cache budget holds one lint set + one wasm set + the small
/// rail-evidence and architect-xtask entries instead of being evicted by
/// every open PR. `cache-on-failure`: the cache holds compiled dependencies,
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
        // WarpBuild's cache (the jobs run there): unlimited, 7 days from
        // last use, instead of GitHub's 10 GB.
        .with("cache-provider", "warpbuild")
        .with("cache-on-failure", true)
        .with("cache-bin", false),
        Step::uses(
            "Prune unused units before the cache is saved",
            "./.github/actions/prune-cargo-target",
        )
        .with("target-dir", "target"),
    ]
}

/// Garbage collection for a PERSISTENT target dir (the test jobs), which
/// cargo never does: Dioxus's prune-cargo-target split in two.
/// [`target_mark`] sets every fingerprint's atime to 0 before the build;
/// [`target_sweep`] then deletes every unit whose fingerprint the build did
/// not read — what earlier branches' lockfiles, features and profiles left.
///
/// The sweep runs only after a FULL (`--workspace`) build succeeded
/// ([`nextest`]'s `prunable`): after a narrowed PR build (`-p …`) it would
/// delete the rest of the workspace and turn the next run cold. Full runs —
/// main's pushes and the nightly — prune to the workspace's live set, which
/// also clears whatever PR builds added since. A red test run still sweeps:
/// its build was fine.
pub fn target_mark() -> Step {
    Step::uses(
        "Mark the target dir's fingerprints",
        "./.github/actions/prune-cargo-target/mark",
    )
    .with("target-dir", "${{ env.CARGO_TARGET_DIR }}")
}

/// See [`target_mark`].
pub fn target_sweep() -> Step {
    Step::uses(
        "Prune what the build did not use",
        "./.github/actions/prune-cargo-target/sweep",
    )
    .cond("${{ !cancelled() && steps.nextest.outputs.prunable == 'true' }}")
    .with("target-dir", "${{ env.CARGO_TARGET_DIR }}")
}

/// apps/session/web/assets/tailwind.css is gitignored build output that
/// `asset!()` demands at compile time (see apps/session/web/tailwind.css) —
/// the sheet has to exist before any cargo step touches session-web. Every
/// job that compiles session-web runs this itself: the jobs share no
/// checkout.
pub fn tailwind(tc: Toolchain) -> Step {
    Step::run("Web tailwind sheet", &tc.cmd("just web-tailwind")).cond("${{ !cancelled() }}")
}

/// features/ui/plugin-faces/generated is gitignored build output too: the
/// plugin faces as `.frame` documents, which session-daw reads at runtime
/// (`faces::design_dir()`), and its faces tests with it — without them no
/// face opens and those tests fail. session-daw does not depend on
/// plugin-faces, so its tests cannot generate them; the job does, before
/// nextest. A release build of plugin-faces into the persistent target dir:
/// minutes once, then a fresh binary and a few seconds of writing.
pub fn faces(tc: Toolchain) -> Step {
    Step::run("Plugin faces", &tc.cmd("just faces")).cond("${{ !cancelled() }}")
}

/// What this kind would put in the Actions cache (rust-cache trims the
/// workspace crates before saving, so the entry is smaller), and the disk
/// left: the numbers to watch against the 10 GB budget.
pub fn footprint() -> Step {
    Step::run(
        "Cache footprint",
        "\
du -sh target ~/.cargo/registry ~/.cargo/git 2>/dev/null || true
df -h / | tail -n 1
",
    )
    .cond("${{ always() }}")
}
