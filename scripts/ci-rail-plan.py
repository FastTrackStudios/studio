#!/usr/bin/env python3
"""Turn a `cargo rail plan --json` (contract v9) into checks.yml's job gates.

Usage: ci-rail-plan.py PLAN.json  -> prints KEY=VALUE lines for $GITHUB_OUTPUT

Outputs:
  full=true|false    the whole workspace (a workspace-scoped selection, or
                     anything this script can't read exactly)
  cargo=true|false   any compiling work at all (check, clippy, tests)
  check=true|false   the check job has work (cargo.build is required)
  check_packages=A B what it checks: the affected packages, space-separated
                     ("" with check=true = the whole workspace)
  tests=true|false   the tests job has work
  test_filter=EXPR   nextest filterset for the selected packages ("" = all)
  test_packages=A B  the packages the tests job builds and runs ("" = the
                     whole workspace)
  wasm=true|false    the wasm job has work
  n_packages=N       size of the test selection (for the log)

Tests come from the `test-data` work item (.config/rail.toml): cargo.test's
selection plus the packages owning any changed fixture/golden/snapshot,
which compiler evidence cannot see. A change to CI's own files (`ci-infra`)
adds a SMOKE run of the tests job on one tiny package.

The cargo gate counts cargo.build, cargo.clippy and cargo.test only:
cargo.doc, cargo.doctest and cargo.package have no evidence recorder, so
they always widen, and would make every docs-only PR compile everything.

Anything unexpected fails open (full=true), never narrow on a guess.
"""
import json
import sys
from typing import NoReturn

# Leaf entry points of the browser builds the wasm job checks: the three web
# bundles (`just web-wasm-check`) and the Signal browser
# remote (studio-desktop's web build). rail's selection already includes
# reverse dependencies, so membership of these is enough.
WASM = {"session-daw-web", "session-web", "signal-web", "studio-desktop"}

# A change to CI's own definition (.config/rail.toml `ci-infra`): smoke the
# tests job end to end on the workflows crate — tiny, and CI's own code.
SMOKE_WORK = "ci-infra"
SMOKE_PACKAGE = "xtask-workflows"

COMPILING = {"cargo.build", "cargo.clippy", "cargo.test"}

# The plan JSON contract this reads. Anything else fails open: a new rail
# could rename what this relies on.
CONTRACT = 9


def emit(**kv):
    for k, v in kv.items():
        if isinstance(v, bool):
            v = str(v).lower()
        print(f"{k}={v}")


def full(reason) -> NoReturn:
    print(f"plan: {reason} -> full run", file=sys.stderr)
    emit(full=True, cargo=True, check=True, check_packages="", tests=True, test_filter="",
         test_packages="", wasm=True, n_packages="all")
    sys.exit(0)


def selection(item):
    """The packages a required Cargo work item covers: a set, or None for the
    whole workspace (or anything this can't read exactly)."""
    scope = item.get("scope") or {}
    if scope.get("kind") != "cargo":
        return None
    sel = scope.get("selection") or {}
    if sel.get("kind") == "workspace":
        return None
    named = {p.get("name") for p in sel.get("packages") or [] if p.get("name")}
    if named:
        return named
    # Older shape: the package flags in cargo_args.
    names = set()
    it = iter(sel.get("cargo_args") or [])
    for a in it:
        if a in ("-p", "--package"):
            names.add(next(it, ""))
        elif a.startswith("--package="):
            names.add(a.split("=", 1)[1])
    names.discard("")
    return names or None


def required(work, name):
    return (work.get(name) or {}).get("state") == "required"


def main():
    try:
        plan = json.load(open(sys.argv[1]))
    except Exception as e:  # noqa: BLE001 - any read failure fails open
        full(f"unreadable plan ({e})")
    if plan.get("error") or "work" not in plan or "required" not in plan:
        full("plan reported an error")
    if plan.get("plan_contract_version") != CONTRACT:
        full(f"plan contract {plan.get('plan_contract_version')!r}, this reads {CONTRACT}")
    work = plan["work"]
    req = set(plan["required"])

    cargo = bool(req & COMPILING)

    check = required(work, "cargo.build")
    built = selection(work["cargo.build"]) if check else set()
    check_packages = "" if built is None else " ".join(sorted(built))

    # Tests: the test-data item (cargo.test + runtime fixtures), else cargo.test.
    source = "test-data" if "test-data" in work else "cargo.test"
    tested = selection(work[source]) if required(work, source) else set()
    if tested is None:
        full(f"{source} has workspace scope")

    if SMOKE_WORK in req and not tested:
        print("plan: CI inputs changed -> smoke the tests job", file=sys.stderr)
        tested = {SMOKE_PACKAGE}

    # The browser builds: whenever a selection reaches one of them (or the
    # check covers the whole workspace).
    wasm = built is None or bool(((built or set()) | tested) & WASM)

    if not tested:
        print("plan: no tests affected", file=sys.stderr)
        emit(full=False, cargo=cargo, check=check, check_packages=check_packages, tests=False,
             test_filter="", test_packages="", wasm=wasm, n_packages=0)
        return

    # Exact matches: `package(name)` is a glob in nextest, and `name@version`
    # (two packages sharing a name) would match nothing.
    expr = " | ".join(f"package(={p})" for p in sorted(tested))
    emit(full=False, cargo=True, check=check, check_packages=check_packages, tests=True,
         test_filter=expr, test_packages=" ".join(sorted(tested)), wasm=wasm,
         n_packages=len(tested))
    print(f"plan: tests for {len(tested)} package(s): {', '.join(sorted(tested))}", file=sys.stderr)


if __name__ == "__main__":
    main()
