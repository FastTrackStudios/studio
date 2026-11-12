#!/usr/bin/env python3
"""Move every Architect pin, and Keyflow's with it, to one new release.

    just bump-architect v0.12.5                    # keyflow: the newest tag on v0.12.5, if any
    just bump-architect v0.12.5 --keyflow v0.5.4   # keyflow named
    just bump-architect v0.12.5 --keyflow keep     # keyflow left where it is

Studio pins Architect AND Keyflow by tag, and Keyflow pins Architect by tag
itself, so both must agree on one Architect: `Cargo.lock` must hold one
architect source at the new tag (task@main's own older one aside).

What it changes, and nothing else:
  - every `tag = "<old>"` on a FastTrackStudios/architect (and keyflow) git
    dependency in every tracked Cargo.toml;
  - the old tag where a comment names Architect (or Keyflow) on the same
    line, in the manifests and .config/duplicate-deps.toml's notes;
  - Cargo.lock, by letting cargo resolve the edited manifests
    (`cargo metadata`: only what the edit requires moves);
  - .config/duplicate-deps.toml, ADD-ONLY: a crate the bump gives a new
    version gets a line (or its count raised), with a note; nothing is
    lowered, removed or rewritten (`check-duplicate-deps.py --add`);
  - a change file, .changes/studio-architect-on-architect-<hash>.md (patch).

Exit status:
  0  mechanical: only the above. The bot may merge it once CI is green.
  3  done, but needs a person: Architect moved a pin Studio follows (the
     editor, dioxus, blitz, ...), or an architect source other than the
     new tag is left in Cargo.lock. The reasons go to stderr and --report.
  4  waiting on Keyflow: no keyflow tag pins the new Architect yet, so the
     lock would hold two architect sources. Nothing is left changed.
  1  refused (not a stable tag, a downgrade, split pins, the fts-dev link on).

Already on the tag (and keyflow too): exit 0, nothing changed.
The CI bot (xtask/workflows/src/bump_architect.rs) runs exactly this.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import urllib.request
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ARCHITECT = "FastTrackStudios/architect"
KEYFLOW = "FastTrackStudios/keyflow"
STABLE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")
GIT_PIN = re.compile(
    r'git = "https://(?:www\.)?github\.com/([^"]+?)(?:\.git)?"[^}\n]*?\b(?:rev|tag|branch) = "([^"]+)"'
)
# Repos whose pins follow Architect's own (its Cargo.toml says which revision).
FOLLOWED = ("dioxuslabs/dioxus", "fasttrackstudios/blitz", "fasttrackstudios/editor",
            "fasttrackstudios/vendor", "fasttrackstudios/lucide",
            "fasttrackstudios/dioxus-components", "fasttrackstudios/lumen-blocks")
NOTES = (".config/duplicate-deps.toml",)
CHANGE_CRATE = "studio-architect"


def tag_pin(repo: str) -> re.Pattern[str]:
    return re.compile(r'git = "https://(?:www\.)?github\.com/' + repo + r'"[^}\n]*?\btag = "(v[^"]+)"')


def version(tag: str) -> tuple[int, int, int]:
    m = STABLE.match(tag)
    if not m:
        sys.exit(f"bump-architect: {tag!r} is not a stable release tag (vX.Y.Z)")
    return tuple(int(x) for x in m.groups())  # type: ignore[return-value]


def tracked(*patterns: str) -> list[Path]:
    out = subprocess.run(["git", "ls-files", "-z", "--", *patterns], cwd=ROOT,
                         check=True, capture_output=True, text=True).stdout
    return [ROOT / p for p in out.split("\0") if p]


def manifests() -> list[Path]:
    return tracked("Cargo.toml", "*/Cargo.toml")


def current(repo: str) -> str:
    found = {m.group(1) for p in manifests() for m in tag_pin(repo).finditer(p.read_text())}
    if len(found) != 1:
        sys.exit(f"bump-architect: the {repo} pins disagree ({', '.join(sorted(found)) or 'none'}); "
                 "make them one tag by hand first")
    return found.pop()


def pins(text: str) -> dict[str, set[str]]:
    found: dict[str, set[str]] = defaultdict(set)
    for repo, ref in GIT_PIN.findall(text):
        found[repo.lower()].add(ref)
    return found


def fetch(url: str) -> str | None:
    try:
        with urllib.request.urlopen(url, timeout=60) as r:
            return r.read().decode()
    except Exception:  # noqa: BLE001 - missing is the only answer that matters
        return None


def manifest_at(repo: str, tag: str) -> str | None:
    return fetch(f"https://raw.githubusercontent.com/{repo}/refs/tags/{tag}/Cargo.toml")


def remote_tags(repo: str) -> list[str]:
    out = subprocess.run(["git", "ls-remote", "--tags", "--refs", f"https://github.com/{repo}"],
                         check=True, capture_output=True, text=True).stdout
    tags = [line.split("refs/tags/", 1)[1] for line in out.splitlines() if "refs/tags/" in line]
    return sorted((t for t in tags if STABLE.match(t)), key=version, reverse=True)


def on_main(repo: str, tag: str) -> bool:
    """Is `tag`'s commit on `repo`'s main (main contains it)?"""
    req = urllib.request.Request(f"https://api.github.com/repos/{repo}/compare/main...{tag}",
                                 headers={"Accept": "application/vnd.github+json"})
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return json.load(r).get("status") in ("behind", "identical")
    except Exception as e:  # noqa: BLE001
        sys.exit(f"bump-architect: cannot tell whether {repo} {tag} is on main ({e})")


def keyflow_on(architect: str, at_least: str) -> str | None:
    """The newest keyflow release (not older than `at_least`) pinning `architect`,
    whose commit is on keyflow's main: a tag pushed onto a branch nobody
    merged is never picked by `auto` (a person can still name it)."""
    for tag in remote_tags(KEYFLOW):
        if version(tag) < version(at_least):
            return None
        text = manifest_at(KEYFLOW, tag) or ""
        if {m.group(1) for m in tag_pin(ARCHITECT).finditer(text)} == {architect}:
            if on_main(KEYFLOW, tag):
                return tag
            print(f"bump-architect: keyflow {tag} pins architect {architect} but is not on keyflow's main: skipped",
                  file=sys.stderr)
    return None


def token(tag: str) -> re.Pattern[str]:
    # The tag as a whole token: v0.12.3 but not v0.12.30 or v0.12.3.1.
    return re.compile(r"(?<![\w.])" + re.escape(tag) + r"(?![\w])(?!\.\d)")


def rewrite(path: Path, old: str, new: str, word: str) -> bool:
    """`old` -> `new` on the lines of `path` that mention `word` (any case)."""
    text = path.read_text()
    tok = token(old)
    out = "".join(tok.sub(new, line) if word in line.lower() else line
                  for line in text.splitlines(keepends=True))
    if out != text:
        path.write_text(out)
        return True
    return False


def followed_moves(ours: str, arch_old: str, arch_new: str) -> list[str]:
    """Pins Studio shares with Architect that Architect moved between the tags."""
    mine, before, after = pins(ours), pins(arch_old), pins(arch_new)
    moves = []
    for repo in FOLLOWED:
        stale = (mine.get(repo, set()) & before.get(repo, set())) - after.get(repo, set())
        if stale:
            now = sorted(after.get(repo, set()) - before.get(repo, set())) or ["(dropped)"]
            moves.append(f"{repo}: architect moved {', '.join(sorted(stale))} -> {', '.join(now)}; "
                         "move Studio's pin with it (and whatever pins the same revision)")
    return moves


def lock_architect_sources() -> dict[str, list[str]]:
    """architect tag -> the lock's packages that depend on that source."""
    lock = (ROOT / "Cargo.lock").read_text()
    users: dict[str, list[str]] = defaultdict(list)
    for block in lock.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        for tag in re.findall(r'FastTrackStudios/architect\?tag=(v[^)#"]+)\)', block):
            users[tag].append(name.group(1) if name else "?")
    for tag in re.findall(r'source = "git\+https://(?:www\.)?github\.com/FastTrackStudios/architect\?tag=(v[^#"]+)#', lock):
        users.setdefault(tag, [])
    return users


def change_file(arch: str, keyflow: str | None) -> Path:
    slug = hashlib.sha1(f"{arch} {keyflow}".encode()).hexdigest()[:4]
    path = ROOT / ".changes" / f"studio-architect-on-architect-{slug}.md"
    with_kf = f" and keyflow {keyflow}" if keyflow else ""
    path.write_text(
        f'---\n"{CHANGE_CRATE}" = "patch"\n---\n\n'
        f"Changed Studio Architect to architect {arch}{with_kf}: what that release carries is in its notes, "
        f"https://github.com/{ARCHITECT}/releases/tag/{arch} (studio)\n"
    )
    return path


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("tag", help="the Architect release tag, vX.Y.Z")
    ap.add_argument("--keyflow", default="auto",
                    help="keyflow tag, `auto` (the newest on the new architect) or `keep`")
    ap.add_argument("--report", type=Path, help="write the reasons a person is needed here")
    ap.add_argument("--allow-downgrade", action="store_true")
    ap.add_argument("--no-lock", action="store_true", help="leave Cargo.lock alone (tests)")
    ap.add_argument("--no-change-file", action="store_true")
    args = ap.parse_args()

    root_manifest = ROOT / "Cargo.toml"
    if re.search(r"^# >>> fts-dev link", root_manifest.read_text(), re.M):
        sys.exit("bump-architect: the fts-dev link is on (Cargo.toml's `# >>> fts-dev link` block): "
                 "run this in a clean worktree")

    new = args.tag
    old = current(ARCHITECT)
    kf_old = current(KEYFLOW)
    version(new)
    if version(new) < version(old) and not args.allow_downgrade:
        sys.exit(f"bump-architect: {new} is older than the current pin {old} (--allow-downgrade)")

    if args.keyflow == "keep":
        kf_new = kf_old
    elif args.keyflow == "auto":
        kf_new = keyflow_on(new, kf_old) or kf_old
    else:
        kf_new = args.keyflow
        version(kf_new)
    if old == new and kf_old == kf_new:
        print(f"bump-architect: already on architect {new}, keyflow {kf_new}")
        return 0

    arch_old, arch_new = manifest_at(ARCHITECT, old), manifest_at(ARCHITECT, new)
    if arch_new is None:
        sys.exit(f"bump-architect: no {ARCHITECT} release {new} (its Cargo.toml is not there)")
    kf_text = manifest_at(KEYFLOW, kf_new)
    if kf_text is None:
        sys.exit(f"bump-architect: no {KEYFLOW} release {kf_new}")
    kf_arch = {m.group(1) for m in tag_pin(ARCHITECT).finditer(kf_text)}
    if kf_arch != {new}:
        msg = (f"keyflow {kf_new} pins architect {', '.join(sorted(kf_arch))}, not {new}, and no keyflow "
               f"release pins {new} yet: bump and tag keyflow first (`just bump-architect {new} --version ...` there)")
        print(f"bump-architect: waiting on keyflow: {msg}", file=sys.stderr)
        if args.report:
            args.report.write_text(f"Waiting on keyflow: {msg}\n")
        return 4

    moves = followed_moves(root_manifest.read_text(), arch_old or "", arch_new)
    changed: set[Path] = set()
    for path in [*manifests(), *tracked(*NOTES)]:
        if old != new and rewrite(path, old, new, "architect"):
            changed.add(path)
        if kf_old != kf_new and rewrite(path, kf_old, kf_new, "keyflow"):
            changed.add(path)

    problems = list(moves)
    if not args.no_lock:
        before = set(lock_architect_sources())
        subprocess.run(["cargo", "metadata", "--format-version", "1"], cwd=ROOT, check=True,
                       stdout=subprocess.DEVNULL)
        after = lock_architect_sources()
        # Allowed: the new tag, and sources the bump never pinned (task@main's own).
        for tag, users in sorted(after.items()):
            if tag != new and (tag == old or tag not in before):
                problems.append(f"Cargo.lock holds architect {tag} beside {new} "
                                f"(depended on by {', '.join(sorted(set(users))) or 'nothing named'})")
        dup = subprocess.run([sys.executable, str(ROOT / "scripts/check-duplicate-deps.py"), "--add",
                              f"architect {new}" + (f", keyflow {kf_new}" if kf_new != kf_old else "")],
                             cwd=ROOT, capture_output=True, text=True)
        print(dup.stdout, end="")
        if dup.returncode != 0:
            print(dup.stderr, end="", file=sys.stderr)
            sys.exit("bump-architect: check-duplicate-deps.py --add failed")
        if "added" in dup.stdout or "raised" in dup.stdout:
            changed.add(ROOT / NOTES[0])

    if not args.no_change_file:
        changed.add(change_file(new, kf_new if kf_new != kf_old else None))

    for path in sorted(changed):
        print(f"  {path.relative_to(ROOT)}")
    print(f"bump-architect: architect {old} -> {new}, keyflow {kf_old} -> {kf_new}")
    if problems:
        msg = "Needs a person:\n" + "\n".join(f"- {p}" for p in problems)
        print(msg, file=sys.stderr)
        if args.report:
            args.report.write_text(msg + "\n")
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
