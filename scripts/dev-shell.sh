#!/usr/bin/env bash
# The Justfile's shell: every recipe runs in this checkout's dev shell
# (devenv.nix), so cargo is the pinned 1.99.1 with the fleet's kache
# environment. The PATH outside it has carried a cargo 1.98.1, whose builds
# share no kache keys with anyone's.
#
#   scripts/dev-shell.sh -c '<command>' [args...]   (just's `shell`)
#   scripts/dev-shell.sh <script> [args...]         (just's `script-interpreter`)
#
# Already in this checkout's shell (direnv, `devenv shell`): bash directly,
# with nothing to enter again. Anywhere else: `devenv shell --` first.
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ "${DEVENV_ROOT:-}" = "$root" ] && [ -n "${DEVENV_PROFILE:-}" ]; then
    exec bash -euo pipefail "$@"
fi
exec devenv shell -- bash -euo pipefail "$@"
