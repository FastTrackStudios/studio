# frame — run inside `nix develop` (the flake provides the toolchain, the
# GUI libraries and the Vulkan loader the renders need).

default:
    @just --list

# Every crate and target, the configuration CI checks.
check:
    cargo check --workspace --all-targets

# The workspace's tests.
test:
    cargo test --workspace

fmt:
    cargo fmt --all

# The plugin faces (examples/plugin-faces), generated, not kept in git.
#   just faces            # all of them
#   just faces comp_fet   # one set
faces *ONLY:
    cargo run -q --release -p plugin-faces -- examples/plugin-faces/generated {{ if ONLY != "" { "--only " + ONLY } else { "" } }}

# The face library: every plugin face rendered with the GPU effects stage
# to OUT/<set>/<face>.webp, indexed by kind in OUT/index.json. Run
# `just faces` first; needs ImageMagick.
#   just faces-gallery /tmp/gallery --only saturate
faces-gallery OUT *ARGS:
    cargo build --release -p frame-cli --features anyrender --bin render_surface
    python3 scripts/face_gallery.py {{OUT}} {{ARGS}}

# Figma parity: import → render → pixel + SVG diff against Figma's own
# output (crates/frame/render/tests/parity, xtask/scripts).
parity *ARGS="--all":
    cargo xtask parity_test {{ARGS}}

# Replay Studio commits that touched Frame after the cut (until Studio
# depends on Frame by tag). See scripts/frame-catch-up.sh.
catch-up STUDIO *ARGS:
    scripts/frame-catch-up.sh {{STUDIO}} {{ARGS}}
