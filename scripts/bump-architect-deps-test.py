#!/usr/bin/env python3
"""bump-architect.py's check of the repos Studio pins that pin Architect
themselves (daw, fts-fx, ...), against fixtures (`just release-bots-test`,
PR checks). Hermetic: the reader is a dict, nothing touches the network.

A bump of Studio alone would leave a second architect source in Cargo.lock
whenever such a repo still pins Studio's current tag on the plain URL; the
script must name those repos and wait (exit 4) before changing anything."""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location("bump", Path(__file__).with_name("bump-architect.py"))
assert spec and spec.loader
bump = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bump)

OLD, NEW = "v0.12.6", "v0.12.7"
DAW, FX = "3fdf2d6fa78a07e29380d7a486f207f1cc4f6f7f", "5bae622f5d874ce58ab04916ab29a8100e883012"

STUDIO = f'''
architect = {{ git = "https://github.com/FastTrackStudios/architect", tag = "{OLD}" }}
keyflow = {{ git = "https://github.com/FastTrackStudios/keyflow", tag = "v0.5.4" }}
daw-proto = {{ git = "https://github.com/FastTrackStudios/daw", rev = "{DAW}" }}
dsp = {{ git = "https://github.com/FastTrackStudios/fts-fx", rev = "{FX}" }}
fts-sample = {{ git = "https://www.github.com/FastTrackStudios/daw", rev = "{DAW}" }}
editor = {{ git = "https://www.github.com/FastTrackStudios/editor", rev = "6817dc2c709f" }}
task = {{ git = "https://github.com/FastTrackStudios/task", branch = "main" }}
nice-plug = {{ git = "https://github.com/FastTrackStudios/nice-plug.git", branch = "fts-baseview-03" }}
dioxus = {{ git = "https://github.com/DioxusLabs/dioxus", rev = "430cfb56" }}
'''


def arch(tag: str, www: bool = False) -> str:
    host = "www.github.com" if www else "github.com"
    return f'architect = {{ git = "https://{host}/FastTrackStudios/architect", tag = "{tag}" }}\n'


def reader(files: dict[tuple[str, str], str | None]):
    seen: list[tuple[str, str]] = []

    def read(repo: str, ref: str, path: str) -> str | None:
        assert path == "Cargo.toml", path
        seen.append((repo, ref))
        return files.get((repo, ref), "[workspace]\n")
    read.seen = seen  # type: ignore[attr-defined]
    return read


fail = 0


def check(what: str, files: dict, want_waits: list[str], want_problems: int = 0, manifests=(STUDIO,)):
    global fail
    read = reader(files)
    waits, problems = bump.dependents_behind(list(manifests), OLD, NEW, read)
    named = sorted(w.split(" ", 1)[0] for w in waits)
    ok = named == sorted(want_waits) and len(problems) == want_problems
    print(f"{'ok  ' if ok else 'FAIL'} {what}" + ("" if ok else f" (waits {named}, problems {problems})"))
    if not ok:
        fail = 1
    return read


both_behind = {("FastTrackStudios/daw", DAW): arch(OLD), ("FastTrackStudios/fts-fx", FX): arch(OLD)}
r = check("daw and fts-fx on Studio's tag: both named, nothing else", both_behind,
          ["FastTrackStudios/daw", "FastTrackStudios/fts-fx"])
reads = r.seen  # type: ignore[attr-defined]
if any(repo.lower() in ("fasttrackstudios/architect", "fasttrackstudios/keyflow", "fasttrackstudios/editor")
       or not repo.startswith("FastTrackStudios/") for repo, _ in reads):
    print(f"FAIL architect, keyflow, the followed repos and other orgs are never read ({reads})")
    fail = 1
elif sorted(reads).count(("FastTrackStudios/daw", DAW)) != 1:
    print(f"FAIL daw read once for both spellings ({reads})")
    fail = 1
else:
    print("ok   architect, keyflow, followed repos and other orgs skipped; daw read once for both spellings")

check("fts-fx already on the new tag: only daw waits",
      {("FastTrackStudios/daw", DAW): arch(OLD), ("FastTrackStudios/fts-fx", FX): arch(NEW)},
      ["FastTrackStudios/daw"])
check("both already on the new tag: no wait",
      {("FastTrackStudios/daw", DAW): arch(NEW), ("FastTrackStudios/fts-fx", FX): arch(NEW)}, [])
check("task@main on its own older tag (a separate source already): no wait",
      {("FastTrackStudios/task", "main"): arch("v0.10.0")}, [])
check("a dependent on the www spelling (Studio's www [patch] reaches it): no wait",
      {("FastTrackStudios/daw", DAW): arch(OLD, www=True)}, [])
check("a dependent whose Cargo.toml cannot be read: a problem for a person, not a wait",
      {("FastTrackStudios/fts-fx", FX): None}, [], want_problems=1)
check("no dependents pinned: no wait", {}, [], manifests=(arch(OLD),))

sys.exit(fail)
