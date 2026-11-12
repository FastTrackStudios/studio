#!/usr/bin/env python3
"""Fail when Cargo.lock gains a second (or third...) version of a crate.

Each extra version of a crate is compiled separately. The ones we have
are listed, with how many versions each may have, in
.config/duplicate-deps.toml; this fails when a crate not on the list gets
a second version, or one on it gets more than its count. A crate that
drops off (or below its count) is reported so the list can shrink.

    python3 scripts/check-duplicate-deps.py          # check
    python3 scripts/check-duplicate-deps.py --write  # rewrite the list from Cargo.lock (drops every note)
    python3 scripts/check-duplicate-deps.py --add "architect v0.12.5"
        # ADD-ONLY: a line for each new duplicate, a raised count for each grown
        # one, each with that reason; every other line and note is kept as is
        # (what the Architect bump bot runs)

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


def add(reason):
    """Raise or add the lines Cargo.lock now needs; touch nothing else."""
    dups = lock_versions()
    text = ALLOW.read_text()
    allowed = tomllib.loads(text)["allowed"]
    lines = text.splitlines(keepends=True)
    start = next(i for i, l in enumerate(lines) if l.strip() == "[allowed]") + 1
    done = []
    for name, versions in sorted(dups.items()):
        cap = allowed.get(name, 1)
        if len(versions) <= cap:
            continue
        entry = f'{name} = {len(versions)}  # {", ".join(versions)} ({reason})\n'
        key = re.compile(rf'^{re.escape(name)}\s*=')
        at = next((i for i in range(start, len(lines)) if key.match(lines[i])), None)
        if at is not None:
            # A note on the old line stays, after the new versions.
            note = lines[at].split("#", 1)[1].strip() if "#" in lines[at] else ""
            if note and not re.fullmatch(r"[0-9A-Za-z.+-]+(, [0-9A-Za-z.+-]+)*", note):
                entry = entry.rstrip("\n") + f"; before: {note}\n"
            lines[at] = entry
            done.append(f"  raised {name}: {cap} -> {len(versions)}")
            continue
        # A new line, in name order among the entries (the list is sorted).
        at = next((i for i in range(start, len(lines))
                   if re.match(r'^[A-Za-z0-9_.-]+\s*=', lines[i]) and lines[i].split("=")[0].strip() > name),
                  len(lines))
        lines.insert(at, entry)
        done.append(f"  added {name} = {len(versions)}")
    if done:
        ALLOW.write_text("".join(lines))
        print(f"{ALLOW.relative_to(ROOT)} ({reason}):")
        print("\n".join(done))
    else:
        print(f"{ALLOW.relative_to(ROOT)}: nothing to add")
    return 0


def main():
    if "--add" in sys.argv:
        i = sys.argv.index("--add")
        reason = sys.argv[i + 1] if i + 1 < len(sys.argv) else "added"
        return add(reason)
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
