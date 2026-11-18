#!/usr/bin/env python3
"""The www [patch] guard, on every PR (PR checks; `just check-www-patch`).

Studio retargets keyflow's www-spelled Architect to its own tag with
`[patch."https://www.github.com/FastTrackStudios/architect"]`. This fails when
  - that table's tags are not exactly Studio's own Architect pin (a bump that
    moved one and not the other would give keyflow another Architect);
  - Cargo.lock holds any architect source on the www spelling (a keyflow
    crate's direct Architect dependency missing from the table);
  - Cargo.lock holds more than one architect source on the plain URL
    besides task@main's own (TASK_ARCHITECT below).
`scripts/bump-architect.py` refuses the same on a bump; this keeps every
other PR honest too.

Between tags, Studio pins Architect by revision. The repos it takes by git
(daw, fts-fx, signal, studio-adapters, session) still name Architect on the
plain URL by their tags, so the plain URL itself is patched instead, to the
www spelling at that revision (`[patch."https://github.com/FastTrackStudios/architect"]`).
Then this fails when
  - Studio's own Architect pins name more than one revision;
  - that table points a crate anywhere but the www spelling at that revision;
  - Cargo.lock holds Architect from any source but that one (besides
    task@main's own).
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# task@main's own Architect, which no patch touches (Cargo.toml, AGENTS.md).
TASK_ARCHITECT = {"v0.10.0"}


PLAIN_HEADER = '[patch."https://github.com/FastTrackStudios/architect"]'


def check_by_rev(manifest: str, lock: str) -> list[str]:
    """The revision-pinned shape (see the module's doc)."""
    bad = []
    head = manifest.split(PLAIN_HEADER)[0]
    own = set(re.findall(r'git = "https://github\.com/FastTrackStudios/architect"[^}\n]*?rev = "([0-9a-f]{40})"', head))
    if len(own) != 1:
        bad.append(f"Studio's own Architect revisions disagree: {sorted(own)}")
    m = re.search(r'^\[patch\."https://github\.com/FastTrackStudios/architect"\]\n((?:(?!\[).*\n)*)', manifest, re.M)
    if not m:
        bad.append(f"no {PLAIN_HEADER} table for the revision pin")
    else:
        patched = set(re.findall(r'rev = "([0-9a-f]{40})"', m.group(1)))
        hosts = set(re.findall(r'git = "(https://[^"]+)"', m.group(1)))
        if patched != own:
            bad.append(f"the plain-URL [patch] table pins {sorted(patched)}, Studio pins {sorted(own)}")
        if hosts != {"https://www.github.com/FastTrackStudios/architect"}:
            bad.append(f"the plain-URL [patch] table must point at the www spelling, points at {sorted(hosts)}")
    sources = set(re.findall(r'source = "git\+(https://(?:www\.)?github\.com/FastTrackStudios/architect\?[^#"]+)', lock))
    allowed = {f"https://www.github.com/FastTrackStudios/architect?rev={r}" for r in own}
    allowed |= {f"https://github.com/FastTrackStudios/architect?tag={t}" for t in TASK_ARCHITECT}
    if sources - allowed:
        bad.append(f"Cargo.lock holds architect from {sorted(sources - allowed)} beside Studio's {sorted(allowed)}")
    return bad


def by_rev(manifest: str) -> bool:
    """Whether Studio pins Architect by revision (between tags)."""
    return PLAIN_HEADER in manifest


def check(manifest: str, lock: str) -> list[str]:
    if by_rev(manifest):
        return check_by_rev(manifest, lock)
    bad = []
    own = set(re.findall(r'git = "https://github\.com/FastTrackStudios/architect"[^}\n]*?tag = "(v[^"]+)"',
                         manifest.split('[patch."https://www.github.com/FastTrackStudios/architect"]')[0]))
    if len(own) != 1:
        bad.append(f"Studio's own Architect pins disagree: {sorted(own)}")
    m = re.search(r'^\[patch\."https://www\.github\.com/FastTrackStudios/architect"\]\n((?:(?!\[).*\n)*)', manifest, re.M)
    if not m:
        bad.append("no [patch.\"https://www.github.com/FastTrackStudios/architect\"] table")
    else:
        patched = set(re.findall(r'tag = "(v[^"]+)"', m.group(1)))
        hosts = set(re.findall(r'git = "(https://[^"]+)"', m.group(1)))
        if patched != own:
            bad.append(f"the www [patch] table pins {sorted(patched)}, Studio pins {sorted(own)}")
        if hosts != {"https://github.com/FastTrackStudios/architect"}:
            bad.append(f"the www [patch] table must point at the plain URL, points at {sorted(hosts)}")
    www = sorted(set(re.findall(r'source = "git\+https://www\.github\.com/FastTrackStudios/architect\?tag=(v[^#"]+)', lock)))
    if www:
        bad.append(f"Cargo.lock holds architect on the www spelling ({', '.join(www)}): add the crate to the www [patch] table")
    plain = set(re.findall(r'source = "git\+https://github\.com/FastTrackStudios/architect\?tag=(v[^#"]+)', lock))
    if plain - TASK_ARCHITECT - own:
        bad.append(f"Cargo.lock holds architect {sorted(plain - TASK_ARCHITECT - own)} beside Studio's {sorted(own)}")
    return bad


HEADER = '[patch."https://www.github.com/FastTrackStudios/architect"]'


def self_test() -> int:
    """A patch table one tag (or revision) behind, and a stray source in
    the lock, must fail; the tree must pass."""
    manifest = (ROOT / "Cargo.toml").read_text()
    lock = (ROOT / "Cargo.lock").read_text()
    if by_rev(manifest):
        head, table = manifest.split(PLAIN_HEADER)
        behind = head + PLAIN_HEADER + re.sub(r'rev = "[0-9a-f]{40}"', 'rev = "' + "0" * 40 + '"', table, count=1)
        stray = lock + '\n[[package]]\nname = "architect-x"\nversion = "0.1.0"\nsource = "git+https://github.com/FastTrackStudios/architect?tag=v0.0.1#00"\n'
        failed = [n for n, (m, l) in {"a stale patch revision": (behind, lock), "a stray source": (manifest, stray)}.items()
                  if not check(m, l)]
        if check(manifest, lock):
            failed.append("the tree as it is")
        if failed:
            print(f"self-test FAILED: {failed}")
            return 1
        print("self-test ok: a stale patch revision and a stray source both fail; the tree passes")
        return 0
    head, table = manifest.split(HEADER)
    behind = head + HEADER + re.sub(r'tag = "v[^"]+"', 'tag = "v0.0.1"', table, count=1)
    www = lock + '\n[[package]]\nname = "architect-x"\nversion = "0.1.0"\nsource = "git+https://www.github.com/FastTrackStudios/architect?tag=v0.0.1#00"\n'
    failed = [n for n, (m, l) in {"a stale patch tag": (behind, lock), "a www source": (manifest, www)}.items()
              if not check(m, l)]
    if check(manifest, lock):
        failed.append("the tree as it is")
    if failed:
        print(f"self-test FAILED: {failed}")
        return 1
    print("self-test ok: a stale patch tag and a www source both fail; the tree passes")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()
    bad = check((ROOT / "Cargo.toml").read_text(), (ROOT / "Cargo.lock").read_text())
    if bad:
        print("the www [patch] guard:")
        print("\n".join(f"  {b}" for b in bad))
        return 1
    if by_rev((ROOT / "Cargo.toml").read_text()):
        print("ok: one Architect revision, the plain URL patched to it, no other source in Cargo.lock")
    else:
        print("ok: one Architect, the www [patch] on it, no www source in Cargo.lock")
    return 0


if __name__ == "__main__":
    sys.exit(main())
