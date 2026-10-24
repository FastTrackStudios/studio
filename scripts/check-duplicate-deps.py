#!/usr/bin/env python3
"""Fail when Cargo.lock gains a second (or third...) version of a crate.

Each extra version of a crate is compiled separately. The ones we have
are listed, with how many versions each may have, in
.config/duplicate-deps.toml; this fails when a crate not on the list gets
a second version, or one on it gets more than its count. A crate that
drops off (or below its count) is reported so the list can shrink.

    python3 scripts/check-duplicate-deps.py          # check
    python3 scripts/check-duplicate-deps.py --write  # rewrite the list from Cargo.lock

To see who pulls a version in: cargo tree --target all -i <name>@<version>
"""
import re
import sys
import tomllib
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "Cargo.lock"
ALLOW = ROOT / ".config" / "duplicate-deps.toml"
HEADER = """\
# Crates Cargo.lock holds in more than one version, and how many each may
# have. Every version is a separate compile. scripts/check-duplicate-deps.py
# (CI's Gates job, `just ci`) fails when one appears or grows; a fix that
# drops one should lower or remove its line here.
#
# Almost all of these are third-party crates pinning old majors (syn 1/2/3,
# windows-*, objc2 0.2/0.3) or the graphics stack's two generations (vello
# 0.9/0.10, winit 0.30/0.31-beta, frame vs the rest). Platform-only ones
# (windows-*, Linux-only) are in the lockfile for every target.
#
# Regenerate after an intended change: python3 scripts/check-duplicate-deps.py --write

[allowed]
"""


def lock_versions():
    text = LOCK.read_text()
    by = defaultdict(set)
    for name, version in re.findall(r'\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"', text):
        by[name].add(version)
    return {n: sorted(v) for n, v in by.items() if len(v) > 1}


def main():
    dups = lock_versions()
    if "--write" in sys.argv:
        lines = [f'{n} = {len(v)}  # {", ".join(v)}' for n, v in sorted(dups.items())]
        ALLOW.write_text(HEADER + "\n".join(lines) + "\n")
        print(f"wrote {ALLOW.relative_to(ROOT)}: {len(dups)} crates")
        return 0
    allowed = tomllib.loads(ALLOW.read_text())["allowed"]
    bad = []
    for name, versions in sorted(dups.items()):
        cap = allowed.get(name, 1)
        if len(versions) > cap:
            bad.append(f"  {name}: {len(versions)} versions ({', '.join(versions)}), allowed {cap}")
    shrunk = [f"  {n}: allowed {c}, now {len(dups.get(n, [None]))}" for n, c in sorted(allowed.items())
              if len(dups.get(n, [None])) < c]
    if shrunk:
        print("fewer duplicates than .config/duplicate-deps.toml allows — lower these (or run with --write):")
        print("\n".join(shrunk))
    if bad:
        print("new duplicate crate versions in Cargo.lock (each is another compile):")
        print("\n".join(bad))
        print("move the new dependent onto the version already there, or if it cannot be avoided, raise")
        print("its count in .config/duplicate-deps.toml with a reason. Who pulls it in:")
        print("  cargo tree --target all -i <name>@<version>")
        return 1
    print(f"ok: {len(dups)} crates with more than one version, none new")
    return 0


if __name__ == "__main__":
    sys.exit(main())
