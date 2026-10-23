#!/usr/bin/env python3
"""Turn a `cargo rail plan --json` (contract v9) into checks.yml's job gates.

Usage: ci-rail-plan.py PLAN.json  -> prints KEY=VALUE lines for $GITHUB_OUTPUT

Outputs:
  full=true|false    run everything (workspace scope, or anything this
                     script can't read exactly). CI's own inputs changing
                     is a smoke run, not a full one (.config/rail.toml).
  cargo=true|false   any Cargo work at all (cargo check, clippy)
  tests=true|false   the test step has work
  test_filter=EXPR   nextest filterset for the affected packages ("" = all)
  test_packages=A B  the affected packages, space-separated ("" = build and
                     test the whole workspace)
  wasm=true|false    the wasm checks have work
  n_packages=N       size of the test selection (for the log)

The tests job builds `-p` for each of test_packages (and runs the
filterset), so a narrowed PR also builds less. The accepted cost: building
a subset of packages can change Cargo's feature unification, which rebuilds
some dependencies with other features and leaves a second artifact set in
the warm target dir. Full runs (test_packages empty) build `--workspace`.

Anything unexpected fails open (full=true), never narrow on a guess.
"""
import json
import sys
from typing import NoReturn

# Leaf entry points of the browser builds checks.yml's `wasm` job checks:
# the three web bundles deploy.yml ships (`just web-wasm-check`) and the
# Signal browser remote (studio-desktop's web build). rail's selection
# already includes reverse dependencies, so membership of these is enough
# to know the wasm job has work.
WASM = {"session-daw-web", "session-web", "signal-web", "studio-desktop"}

# Repository work declared in .config/rail.toml (see the comments there).
# ci-infra: a SMOKE run of the tests job (SMOKE_PACKAGE, end to end) on top
# of whatever crates the change affects; dev-shell and docs: nothing.
SMOKE_WORK = {"ci-infra"}
# The workflows crate: tiny, and it is CI's own definition, so building and
# testing it is the honest smoke of the tests job.
SMOKE_PACKAGE = "xtask-workflows"


def emit(**kv):
    for k, v in kv.items():
        if isinstance(v, bool):
            v = str(v).lower()
        print(f"{k}={v}")


def full(reason) -> NoReturn:
    print(f"plan: {reason} -> full run", file=sys.stderr)
    emit(full=True, cargo=True, tests=True, test_filter="", test_packages="", wasm=True, n_packages="all")
    sys.exit(0)


def packages(work):
    """Package names of a required Cargo work item, or None for workspace scope."""
    scope = work.get("scope") or {}
    sel = scope.get("selection") or {}
    if scope.get("kind") != "cargo" or sel.get("kind") == "workspace":
        return None
    args = sel.get("cargo_args")
    if not isinstance(args, list) or not args:
        return None
    names = set()
    it = iter(args)
    for a in it:
        if a in ("-p", "--package"):
            names.add(next(it, ""))
        elif a.startswith("--package="):
            names.add(a.split("=", 1)[1])
        # Target narrowing (--test/--lib/...) is ignored: the package is a
        # safe superset of its targets.
    names.discard("")
    return names or None


def main():
    try:
        plan = json.load(open(sys.argv[1]))
    except Exception as e:  # noqa: BLE001 - any read failure fails open
        full(f"unreadable plan ({e})")
    if plan.get("error") or "work" not in plan or "required" not in plan:
        full("plan reported an error")

    required = set(plan["required"])
    work = plan["work"]

    smoke = bool(required & SMOKE_WORK)
    if smoke:
        print("plan: CI inputs changed -> smoke the tests job", file=sys.stderr)

    cargo = bool(required & {"cargo.build", "cargo.clippy", "cargo.test", "cargo.doc"})

    if "cargo.test" not in required and smoke:
        emit(
            full=False,
            cargo=cargo,
            tests=True,
            test_filter=f"package({SMOKE_PACKAGE})",
            test_packages=SMOKE_PACKAGE,
            wasm=False,
            n_packages=1,
        )
        return

    if "cargo.test" not in required:
        build = work.get("cargo.build", {})
        if build.get("state") == "required":
            pk = packages(build)
            wasm = pk is None or bool(pk & WASM)
        else:
            wasm = False
        print("plan: no tests affected", file=sys.stderr)
        emit(full=False, cargo=cargo, tests=False, test_filter="", test_packages="", wasm=wasm, n_packages=0)
        return

    pk = packages(work["cargo.test"])
    if pk is None:
        full("cargo.test has workspace scope")

    expr = " | ".join(f"package({p})" for p in sorted(pk))
    emit(
        full=False,
        cargo=True,
        tests=True,
        test_filter=expr,
        test_packages=" ".join(sorted(pk)),
        wasm=bool(pk & WASM),
        n_packages=len(pk),
    )
    print(f"plan: tests for {len(pk)} package(s): {', '.join(sorted(pk))}", file=sys.stderr)


if __name__ == "__main__":
    main()
