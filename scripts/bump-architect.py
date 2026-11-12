#!/usr/bin/env python3
"""Move every Architect pin to one new release, and Keyflow's when it must.

    just bump-architect v0.12.5                    # keyflow: kept, or the newest release that fits
    just bump-architect v0.12.5 --keyflow v0.5.4   # keyflow named (never older than the pin)
    just bump-architect v0.12.5 --keyflow keep     # keyflow left where it is

Studio pins Architect AND Keyflow by tag, and Keyflow pins Architect itself.
Keyflow names its Architect on the `www.github.com` spelling, which Studio's
`[patch."https://www.github.com/FastTrackStudios/architect"]` retargets to
Studio's own tag. So:
  - an Architect PATCH release (v0.12.4 -> v0.12.5) needs no keyflow
    release: Studio moves alone, as long as its keyflow pins Architect on
    the www spelling at the same minor;
  - an Architect MINOR (v0.12 -> v0.13), or a keyflow still on the plain
    spelling, needs a keyflow release on it: `--keyflow auto` takes the
    newest one on keyflow's main, or the script waits (exit 4).
Either way `Cargo.lock` must end with ONE architect source, Studio's tag on
the plain URL (task@main's own older one aside), and none on www.

What it changes, and nothing else:
  - every `tag = "<old>"` on a FastTrackStudios/architect (and keyflow) git
    dependency in every tracked Cargo.toml, the www [patch] table included;
  - the old tag where a comment names Architect (or Keyflow) on the same
    line, in the manifests and .config/duplicate-deps.toml's notes;
  - Cargo.lock, by letting cargo resolve the edited manifests (only what
    the edit requires moves), then moving crates the resolve newly added to
    the versions in Architect's own Cargo.lock at the tag, where it can;
  - .config/duplicate-deps.toml, ADD-ONLY (`check-duplicate-deps.py --add`);
  - a change file, .changes/studio-architect-on-architect-<hash>.md
    (patch), carrying Architect's `Data-Migration:` lines when it has any.

Exit status:
  0  mechanical. The bot may merge it once the full suite is green on it.
  3  done, but needs a person, for any of: Architect moved a pin Studio
     follows (editor, dioxus, blitz, ...); an architect source other than
     the new tag (or one on www) is left in Cargo.lock; Cargo.lock gained a
     git source repository; duplicate-deps grew for a crate Architect's
     (or keyflow's) own lock did not move; a major change or a minor jump
     of more than one; Architect's release declares a data migration, or
     its signed release-facts.json cannot be read ("unknown").
  4  waiting on Keyflow: no keyflow release on main fits the new Architect
     yet. Nothing is left changed.
  1  refused: not a stable tag, a downgrade (Architect or an explicit
     --keyflow), split pins, the fts-dev link on, or a tag whose commit is
     not on its repo's main.

Already there: exit 0, nothing changed.
The CI bot (xtask/workflows/src/bump_architect.rs) runs exactly this.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
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
# The org's release key (public half): what signs every SHA256SUMS.
ORG_RELEASE_KEY = "RWRh9Gk9qaStQMhku4THMidEYrQsmAEsm2dDprUkZxShEtqfIcroM8Od"


def ref_pin(repo: str, www: bool | None = None) -> re.Pattern[str]:
    """A git pin of `repo` by tag or rev (www: only that spelling)."""
    host = {None: r"(?:www\.)?", True: r"www\.", False: ""}[www]
    return re.compile(r'git = "https://' + host + r'github\.com/' + repo + r'"[^}\n]*?\b(?:tag|rev) = "([^"]+)"')


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
    """The one ref (tag or rev) every pin of `repo` names."""
    found = {m.group(1) for p in manifests() for m in ref_pin(repo).finditer(p.read_text())}
    if len(found) != 1:
        sys.exit(f"bump-architect: the {repo} pins disagree ({', '.join(sorted(found)) or 'none'}); "
                 "make them one by hand first")
    return found.pop()


def pins(text: str) -> dict[str, set[str]]:
    found: dict[str, set[str]] = defaultdict(set)
    for repo, ref in GIT_PIN.findall(text):
        found[repo.lower()].add(ref)
    return found


def fetch(url: str, binary: bool = False):
    try:
        with urllib.request.urlopen(url, timeout=60) as r:
            data = r.read()
            return data if binary else data.decode()
    except Exception:  # noqa: BLE001 - missing is the only answer that matters
        return None


def raw(repo: str, ref: str, path: str) -> str | None:
    at = f"refs/tags/{ref}" if STABLE.match(ref) else ref
    return fetch(f"https://raw.githubusercontent.com/{repo}/{at}/{path}")


def github(path: str) -> dict:
    req = urllib.request.Request(f"https://api.github.com/{path}",
                                 headers={"Accept": "application/vnd.github+json"})
    token = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def remote_tags(repo: str) -> list[str]:
    out = subprocess.run(["git", "ls-remote", "--tags", "--refs", f"https://github.com/{repo}"],
                         check=True, capture_output=True, text=True).stdout
    tags = [line.split("refs/tags/", 1)[1] for line in out.splitlines() if "refs/tags/" in line]
    return sorted((t for t in tags if STABLE.match(t)), key=version, reverse=True)


def on_main(repo: str, ref: str) -> bool:
    """Is `ref`'s commit on `repo`'s main (main contains it)?"""
    try:
        return github(f"repos/{repo}/compare/main...{ref}").get("status") in ("behind", "identical")
    except Exception as e:  # noqa: BLE001
        sys.exit(f"bump-architect: cannot tell whether {repo} {ref} is on main ({e})")


def keyflow_fits(text: str, architect: str) -> bool:
    """Does a keyflow manifest work with Studio on `architect`?

    On the www spelling, any architect of the same minor does (Studio's
    patch gives keyflow Studio's tag). On the plain spelling, only the
    exact tag (no patch reaches it)."""
    www = {m.group(1) for m in ref_pin(ARCHITECT, www=True).finditer(text)}
    plain = {m.group(1) for m in ref_pin(ARCHITECT, www=False).finditer(text)}
    if www and not plain:
        return all(STABLE.match(t) and version(t)[:2] == version(architect)[:2] for t in www)
    return (www | plain) == {architect}


def keyflow_for(architect: str, at_least: str) -> str | None:
    """The newest keyflow release on keyflow's main (not older than
    `at_least`) that fits `architect`: a tag pushed onto a branch nobody
    merged is never taken by `auto` (a person can still name it)."""
    for tag in remote_tags(KEYFLOW):
        if STABLE.match(at_least) and version(tag) < version(at_least):
            return None
        if keyflow_fits(raw(KEYFLOW, tag, "Cargo.toml") or "", architect):
            if on_main(KEYFLOW, tag):
                return tag
            print(f"bump-architect: keyflow {tag} fits architect {architect} but is not on keyflow's main: skipped",
                  file=sys.stderr)
    return None


def jump(old: str, new: str) -> str | None:
    """Why moving old -> new is not mechanical, if it is not."""
    (omaj, omin, _), (nmaj, nmin, _) = version(old), version(new)
    if nmaj != omaj:
        return f"a major version change ({old} -> {new}): a person checks it is meant"
    if nmin - omin > 1:
        return f"a minor jump of {nmin - omin} ({old} -> {new}): a person checks it is meant"
    return None


def architect_facts(tag: str) -> tuple[str, list[str]]:
    """("none" | "migration" | "unknown", migrations) from Architect's
    release-facts.json, trusted only through its signed SHA256SUMS."""
    base = f"https://github.com/{ARCHITECT}/releases/download/{tag}"
    sums, sig, facts = fetch(f"{base}/SHA256SUMS"), fetch(f"{base}/SHA256SUMS.minisig"), fetch(f"{base}/release-facts.json", True)
    if not (sums and sig and facts) or not shutil.which("minisign"):
        return "unknown", [f"architect {tag}: no signed release-facts.json (or no minisign here)"]
    lines = sig.splitlines()
    if len(lines) != 4 or lines[2] != f"trusted comment: architect {tag} SHA256SUMS":
        return "unknown", [f"architect {tag}: SHA256SUMS.minisig is not the expected signature"]
    with tempfile.TemporaryDirectory() as d:
        (Path(d) / "S").write_text(sums)
        (Path(d) / "S.minisig").write_text(sig)
        ok = subprocess.run(["minisign", "-V", "-q", "-P", ORG_RELEASE_KEY, "-m", f"{d}/S", "-x", f"{d}/S.minisig"],
                            capture_output=True).returncode == 0
    want = next((l.split()[0] for l in sums.splitlines() if l.split()[-1:] in (["release-facts.json"], ["*release-facts.json"])), None)
    if not ok or want != hashlib.sha256(facts).hexdigest():
        return "unknown", [f"architect {tag}: release-facts.json does not verify against its signed SHA256SUMS"]
    data = json.loads(facts)
    if data.get("repo") != "architect" or data.get("tag") != tag:
        return "unknown", [f"architect {tag}: release-facts.json names another release"]
    if data.get("data_migration") is True:
        return "migration", [str(m) for m in data.get("migrations", [])]
    return ("none", []) if data.get("data_migration") is False else ("unknown", [f"architect {tag}: no data_migration"])


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


def lock_packages(text: str) -> dict[tuple[str, str], str]:
    """(name, version) -> source, for every [[package]] of a Cargo.lock."""
    out = {}
    for block in text.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        ver = re.search(r'^version = "([^"]+)"', block, re.M)
        src = re.search(r'^source = "([^"]+)"', block, re.M)
        if name and ver:
            out[(name.group(1), ver.group(1))] = src.group(1) if src else ""
    return out


def git_repos(packages: dict[tuple[str, str], str]) -> set[str]:
    return {re.sub(r"^git\+https://(www\.)?", "", src).split("?")[0].split("#")[0].lower()
            for src in packages.values() if src.startswith("git+")}


def lock_architect_sources() -> dict[str, list[str]]:
    """"<tag>" (or "www:<tag>") -> the lock's packages depending on that source."""
    lock = (ROOT / "Cargo.lock").read_text()
    users: dict[str, list[str]] = defaultdict(list)
    for block in lock.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        for www, tag in re.findall(r'(www\.)?github\.com/FastTrackStudios/architect\?tag=(v[^)#"]+)\)', block):
            users[("www:" if www else "") + tag].append(name.group(1) if name else "?")
    for www, tag in re.findall(r'source = "git\+https://(www\.)?github\.com/FastTrackStudios/architect\?tag=(v[^#"]+)#', lock):
        users.setdefault(("www:" if www else "") + tag, [])
    return users


def versions_moved(old_lock: str | None, new_lock: str | None) -> set[str]:
    """Crate names whose version set differs between two Cargo.lock texts."""
    def by_name(text: str | None) -> dict[str, set[str]]:
        out: dict[str, set[str]] = defaultdict(set)
        for (name, ver) in lock_packages(text or ""):
            out[name].add(ver)
        return out
    a, b = by_name(old_lock), by_name(new_lock)
    return {n for n in set(a) | set(b) if a.get(n) != b.get(n)}


def seed_from_upstream(before: dict, upstream_lock: str) -> list[str]:
    """New registry crates the resolve added: take Architect's own versions,
    the ones it built and tested at the tag (best effort)."""
    after = lock_packages((ROOT / "Cargo.lock").read_text())
    theirs: dict[str, set[str]] = defaultdict(set)
    for (name, ver), src in lock_packages(upstream_lock).items():
        if src.startswith("registry+"):
            theirs[name].add(ver)
    notes = []
    for (name, ver), src in sorted(after.items()):
        if (name, ver) in before or not src.startswith("registry+") or not theirs.get(name) or ver in theirs[name]:
            continue
        want = max(theirs[name], key=lambda v: [int(x) if x.isdigit() else 0 for x in re.split(r"[.+-]", v)])
        r = subprocess.run(["cargo", "update", "-p", f"{name}@{ver}", "--precise", want], cwd=ROOT,
                           capture_output=True, text=True)
        notes.append(f"{name} {ver} -> {want} (Architect's)" if r.returncode == 0
                     else f"{name} {ver} kept (Architect has {want}; cargo refused --precise)")
    return notes


def change_file(arch: str, keyflow: str | None, migrations: list[str]) -> Path:
    slug = hashlib.sha1(f"{arch} {keyflow}".encode()).hexdigest()[:4]
    path = ROOT / ".changes" / f"studio-architect-on-architect-{slug}.md"
    with_kf = f" and keyflow {keyflow}" if keyflow else ""
    body = (f'---\n"{CHANGE_CRATE}" = "patch"\n---\n\n'
            f"Changed Studio Architect to architect {arch}{with_kf}: what that release carries is in its notes, "
            f"https://github.com/{ARCHITECT}/releases/tag/{arch} (studio)\n")
    if migrations:
        body += "\n" + "".join(f"Data-Migration: architect {arch}: {m}\n" for m in migrations)
    path.write_text(body)
    return path


def report(args, problems: list[str], status: int, head: str) -> int:
    msg = head + "\n" + "\n".join(f"- {p}" for p in problems)
    print(msg, file=sys.stderr)
    if args.report:
        args.report.write_text(msg + "\n")
    return status


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("tag", help="the Architect release tag, vX.Y.Z")
    ap.add_argument("--keyflow", default="auto",
                    help="keyflow tag, `auto` (kept if it fits, else the newest that does) or `keep`")
    ap.add_argument("--report", type=Path, help="write the reasons a person is needed here")
    ap.add_argument("--allow-downgrade", action="store_true")
    ap.add_argument("--no-lock", action="store_true", help="leave Cargo.lock alone (tests)")
    ap.add_argument("--no-change-file", action="store_true")
    ap.add_argument("--no-facts", action="store_true",
                    help="skip Architect's release-facts (replaying releases from before them)")
    args = ap.parse_args()

    root_manifest = ROOT / "Cargo.toml"
    if re.search(r"^# >>> fts-dev link", root_manifest.read_text(), re.M):
        sys.exit("bump-architect: the fts-dev link is on (Cargo.toml's `# >>> fts-dev link` block): "
                 "run this in a clean worktree")

    new = args.tag
    version(new)
    old = current(ARCHITECT)
    kf_old = current(KEYFLOW)
    if version(new) < version(old) and not args.allow_downgrade:
        sys.exit(f"bump-architect: {new} is older than the current pin {old} (--allow-downgrade)")
    kind = "minor" if version(new)[:2] != version(old)[:2] else "patch"

    kf_old_text = raw(KEYFLOW, kf_old, "Cargo.toml") or ""
    if args.keyflow == "keep":
        kf_new = kf_old
    elif args.keyflow == "auto":
        kf_new = kf_old if keyflow_fits(kf_old_text, new) else (keyflow_for(new, kf_old) or kf_old)
    else:
        kf_new = args.keyflow
        version(kf_new)
        if STABLE.match(kf_old) and version(kf_new) < version(kf_old) and not args.allow_downgrade:
            sys.exit(f"bump-architect: keyflow {kf_new} is older than the current pin {kf_old} (--allow-downgrade)")
    if old == new and kf_old == kf_new:
        print(f"bump-architect: already on architect {new}, keyflow {kf_new}")
        return 0

    if old != new and not on_main(ARCHITECT, new):
        sys.exit(f"bump-architect: {ARCHITECT} {new} is not on its main: a tag pushed off main is never taken")
    arch_old, arch_new = raw(ARCHITECT, old, "Cargo.toml"), raw(ARCHITECT, new, "Cargo.toml")
    if arch_new is None:
        sys.exit(f"bump-architect: no {ARCHITECT} release {new} (its Cargo.toml is not there)")
    kf_text = raw(KEYFLOW, kf_new, "Cargo.toml")
    if kf_text is None:
        sys.exit(f"bump-architect: no {KEYFLOW} {kf_new}")
    if not keyflow_fits(kf_text, new):
        need = ("a keyflow release on the www spelling at the same minor" if kind == "patch"
                else f"a keyflow release on architect {new.rsplit('.', 1)[0]}")
        return report(args, [f"keyflow {kf_new} does not fit architect {new} ({kind}); "
                             f"{need} is needed: `just bump-architect {new} --version auto` there, merge, "
                             "`just release-tag`"], 4, "Waiting on keyflow:")

    problems = followed_moves(root_manifest.read_text(), arch_old or "", arch_new)
    if old != new and (why := jump(old, new)):
        problems.append(why)
    migrations: list[str] = []
    if old != new and not args.no_facts:
        state, notes = architect_facts(new)
        if state == "migration":
            migrations = notes
            problems.append(f"architect {new} declares a data migration ({'; '.join(notes)}): "
                            "a person decides when Studio ships it")
        elif state == "unknown":
            problems.append("whether architect " + new + " migrates data is unknown (" + "; ".join(notes) + ")")

    changed: set[Path] = set()
    for path in [*manifests(), *tracked(*NOTES)]:
        if old != new and rewrite(path, old, new, "architect"):
            changed.add(path)
        if kf_old != kf_new and rewrite(path, kf_old, kf_new, "keyflow"):
            changed.add(path)

    if not args.no_lock:
        lock_before = (ROOT / "Cargo.lock").read_text()
        before_pkgs = lock_packages(lock_before)
        before = set(lock_architect_sources())
        subprocess.run(["cargo", "metadata", "--format-version", "1"], cwd=ROOT, check=True,
                       stdout=subprocess.DEVNULL)
        upstream_new = raw(ARCHITECT, new, "Cargo.lock")
        if upstream_new:
            for note in seed_from_upstream(before_pkgs, upstream_new):
                print(f"  seeded: {note}")
        after_pkgs = lock_packages((ROOT / "Cargo.lock").read_text())
        new_repos = git_repos(after_pkgs) - git_repos(before_pkgs)
        if new_repos:
            problems.append("Cargo.lock gained git source repositories it did not have: "
                            + ", ".join(sorted(new_repos)) + " (unreviewed code: a person looks first)")
        # One architect source: the new tag on the plain URL (and sources the
        # bump never pinned, task@main's own); never one on www.
        for src, users in sorted(lock_architect_sources().items()):
            if src.startswith("www:"):
                problems.append(f"Cargo.lock holds architect on the www spelling ({src[4:]}, for "
                                f"{', '.join(sorted(set(users))) or '?'}): add those crates to the www [patch] table")
            elif src != new and (src == old or src not in before):
                problems.append(f"Cargo.lock holds architect {src} beside {new} "
                                f"(depended on by {', '.join(sorted(set(users))) or 'nothing named'})")
        dup = subprocess.run([sys.executable, str(ROOT / "scripts/check-duplicate-deps.py"), "--add",
                              f"architect {new}" + (f", keyflow {kf_new}" if kf_new != kf_old else "")],
                             cwd=ROOT, capture_output=True, text=True)
        print(dup.stdout, end="")
        if dup.returncode != 0:
            print(dup.stderr, end="", file=sys.stderr)
            sys.exit("bump-architect: check-duplicate-deps.py --add failed")
        grown = re.findall(r"^\s+(?:added|raised) ([^\s:=]+)", dup.stdout, re.M)
        if grown:
            changed.add(ROOT / NOTES[0])
            # Expected to move: what Architect's own lock (and keyflow's, when
            # it moves) changed between the two tags.
            expected = versions_moved(raw(ARCHITECT, old, "Cargo.lock"), upstream_new)
            if kf_new != kf_old:
                expected |= versions_moved(raw(KEYFLOW, kf_old, "Cargo.lock"), raw(KEYFLOW, kf_new, "Cargo.lock"))
            stray = sorted(set(grown) - expected)
            if stray:
                problems.append("duplicate-deps grew for crates Architect's own lock did not move: "
                                + ", ".join(stray))

    if not args.no_change_file:
        changed.add(change_file(new, kf_new if kf_new != kf_old else None, migrations))

    for path in sorted(changed):
        print(f"  {path.relative_to(ROOT)}")
    print(f"bump-architect: architect {old} -> {new} ({kind}), keyflow {kf_old} -> {kf_new}")
    if problems:
        return report(args, problems, 3, "Needs a person:")
    return 0


if __name__ == "__main__":
    sys.exit(main())
