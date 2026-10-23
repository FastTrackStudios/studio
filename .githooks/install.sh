#!/usr/bin/env bash
set -euo pipefail

# Point git at this directory for hooks. Run once per clone:
#
#   ./.githooks/install.sh          (or `just install-hooks`)
#
# `core.hooksPath` is repo-global and applies to every worktree, so there is
# nothing to copy and nothing to re-run after `git worktree add`. It is also
# the ONLY thing this script does — the hooks are the tracked files here.
#
# Installing makes local CI (scripts/ci-local.sh) run on every push, which
# posts the `local-ci` status main requires. Uninstall with:
#
#   git config --unset core.hooksPath

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git rev-parse --show-toplevel)"
REL="${HOOK_DIR#"$REPO_ROOT"/}"

chmod +x "$HOOK_DIR"/pre-push
git config core.hooksPath "$REL"
echo "Configured git core.hooksPath=$REL"
