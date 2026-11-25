#!/usr/bin/env python3
"""The monorepo's group rules and its two promises, from boundaries.toml.

    python3 scripts/boundaries.py check         # both checks (`just boundaries`)
    python3 scripts/boundaries.py check --edges # the group rules only (seconds)
    python3 scripts/boundaries.py tighten       # drop allow-list entries that no longer occur
    python3 scripts/boundaries.py packages GROUP... [--tested]
                                                # the members of a group (`just test`)
    python3 scripts/boundaries.py groups        # each member and its group

Run it in the dev shell (`just boundaries` does): it calls `cargo`.

THE GROUP RULES. Every workspace member is in one group, by the directory it
lives in (`[groups]`). A member may declare a path dependency only on a group
its own group lists in `[may-depend-on]`, plus the `[[exception]]`s. Every
declared dependency counts: normal, build and dev, optional or not. An
optional one is a feature away from the build, and a dev one compiles for the
crate's tests. This reads `cargo metadata --no-deps`, so it takes seconds.

THE PROMISE (the engine never waits on the UI). Each member of the
`[promise]` groups, outside `exempt`, is resolved on its own with
`cargo tree -p <member> -e normal` and its default features, the way
architect's `xtask boundaries` resolves a row. The resolved graph must hold
no package of a `forbidden` category:
`dioxus` (dioxus*), `blitz` (blitz*), `app-shell`, and `studio-connect` (any
member of the connect group).

A DENY ROW (`[[deny]]`) is one member's own, stricter rule, carried over
from the repo it came from (frame's core boundary): its graph, resolved with
the row's features, holds no package whose name starts with one of
`forbidden`. No allow-list: a deny row holds today.

THE ALLOW-LIST is today's violations, which the untangling removes
(`[allow.edges]`, `[allow.promise]`). It only gets stricter:
- a violation not on it fails;
- an entry on it that no longer occurs fails too, until it is removed.
  `tighten` removes every such entry and nothing else, so the list never grows
  by a command.
"""
import fnmatch
import json
import os
import subprocess
import sys
import tomllib
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = ROOT / "boundaries.toml"

# Everything from this line on is written by `tighten`; everything above it
# is kept as it is.
ALLOW_MARKER = "# ── Allow-list"

KINDS = {None: "normal", "build": "build", "dev": "dev"}


def die(msg):
    print(f"boundaries: {msg}", file=sys.stderr)
    sys.exit(2)


def load_table():
    with TABLE.open("rb") as f:
        return tomllib.load(f)


def metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def rel(path):
    return os.path.relpath(path, ROOT).replace(os.sep, "/")


def matches(path, pattern):
    """`pattern` is a directory (everything under it) or a glob over directories."""
    if any(c in pattern for c in "*?["):
        return fnmatch.fnmatch(path, pattern) or fnmatch.fnmatch(path, pattern + "/*")
    return path == pattern or path.startswith(pattern.rstrip("/") + "/")


class Workspace:
    def __init__(self, table, meta):
        self.table = table
        self.groups = table.get("groups") or die("boundaries.toml has no [groups]")
        self.members = {}  # name -> {"dir", "group", "id", "deps"}
        unmapped = []
        for p in meta["packages"]:
            d = rel(os.path.dirname(p["manifest_path"]))
            g = self.group_of(d)
            if g is None:
                unmapped.append(f"{p['name']} ({d})")
                continue
            self.members[p["name"]] = {"dir": d, "group": g, "id": p["id"], "deps": p["dependencies"]}
        if unmapped:
            die("members in no group (add their directory to [groups]): " + ", ".join(sorted(unmapped)))
        self.by_dir = {m["dir"]: n for n, m in self.members.items()}

    def group_of(self, d):
        # The longest matching directory wins, so a group can sit inside another.
        best = None
        for g, dirs in self.groups.items():
            for pat in [dirs] if isinstance(dirs, str) else dirs:
                if matches(d, pat) and (best is None or len(pat) > len(best[1])):
                    best = (g, pat)
        return best and best[0]

    def in_group(self, *groups):
        return sorted(n for n, m in self.members.items() if m["group"] in groups)


# ── the group rules ────────────────────────────────────────────────────────

def edge_violations(ws):
    """{member: {dep: "kind, …"}} for every declared path dependency across a group line."""
    may = ws.table.get("may-depend-on") or die("boundaries.toml has no [may-depend-on]")
    exceptions = ws.table.get("exception", [])
    found = {}
    for name, m in sorted(ws.members.items()):
        allowed = may.get(m["group"])
        if allowed is None:
            die(f"group {m['group']!r} has no line in [may-depend-on]")
            return found
        for dep in m["deps"]:
            if not dep.get("path"):
                continue
            dep_dir = rel(dep["path"])
            dep_name = ws.by_dir.get(dep_dir)
            if dep_name is None:
                continue  # a path outside the workspace (a [patch] checkout)
            dep_group = ws.members[dep_name]["group"]
            if dep_group in allowed or "*" in allowed:
                continue
            if any(matches(m["dir"], e["from"]) and matches(dep_dir, e["to"]) for e in exceptions):
                continue
            kind = KINDS.get(dep.get("kind")) or str(dep.get("kind"))
            if dep.get("optional"):
                kind += ", optional"
            found.setdefault(name, {}).setdefault(dep_name, set()).add(kind)
    return found


# ── the promise ────────────────────────────────────────────────────────────

def category(line, ws):
    """The forbidden category of one `cargo tree --format {p}` line, or None."""
    parts = line.split(" ", 2)
    name = parts[0]
    if name.startswith("dioxus"):
        return "dioxus"
    if name.startswith("blitz"):
        return "blitz"
    if name == "app-shell":
        return "app-shell"
    src = parts[2].strip().strip("()") if len(parts) > 2 else ""
    if src.startswith("/"):
        member = ws.by_dir.get(rel(src))
        if member and ws.members[member]["group"] == "connect":
            return "studio-connect"
    return None


def promise_members(ws):
    cfg = ws.table.get("promise") or {}
    exempt = cfg.get("exempt", [])
    return [n for n in ws.in_group(*cfg.get("groups", []))
            if not any(matches(ws.members[n]["dir"], e) for e in exempt)]


def tree(ws, name):
    r = subprocess.run(
        ["cargo", "tree", "-q", "-p", ws.members[name]["id"], "-e", "normal",
         "--prefix", "none", "--format", "{p}"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if r.returncode != 0:
        return name, None, r.stderr.strip().splitlines()[-1:] or ["cargo tree failed"]
    cats = {c for line in r.stdout.splitlines() if (c := category(line, ws))}
    return name, cats, None


def promise_violations(ws):
    forbidden = set((ws.table.get("promise") or {}).get("forbidden", []))
    names = promise_members(ws)
    jobs = int(os.environ.get("BOUNDARIES_JOBS", max(2, (os.cpu_count() or 4) // 2)))
    found, errors = {}, {}
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        for name, cats, err in pool.map(lambda n: tree(ws, n), names):
            if err:
                errors[name] = err
            elif cats & forbidden:
                found[name] = cats & forbidden
    return found, errors, len(names)


def deny_violations(ws):
    """[(row label, sorted offending packages)] for each [[deny]] row that fails."""
    bad = []
    for row in ws.table.get("deny", []):
        name = row["package"]
        if name not in ws.members:
            bad.append((name, ["(no such member)"]))
            continue
        cmd = ["cargo", "tree", "-q", "-p", ws.members[name]["id"], "-e", "normal",
               "--prefix", "none", "--format", "{p}"]
        if row.get("all-features"):
            cmd.append("--all-features")
        r = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
        if r.returncode != 0:
            bad.append((name, ["cargo tree failed: " + (r.stderr.strip().splitlines() or ["?"])[-1]]))
            continue
        hits = sorted({line.split(" ", 1)[0] for line in r.stdout.splitlines()
                       if any(line.startswith(f) for f in row["forbidden"])})
        if hits:
            bad.append((name, hits))
    return bad


# ── the ratchet ────────────────────────────────────────────────────────────

def compare(found, allowed, what):
    """(new violations, stale allow entries), each a list of lines."""
    new, stale = [], []
    for pkg, items in sorted(found.items()):
        ok = set(allowed.get(pkg, []))
        for item in sorted(set(items) - ok):
            detail = f" ({', '.join(sorted(items[item]))})" if isinstance(items, dict) else ""
            new.append(f"  {pkg} -> {item}{detail}")
    for pkg, items in sorted(allowed.items()):
        for item in sorted(set(items) - set(found.get(pkg, ()))):
            stale.append(f"  {pkg} -> {item}")
    if new:
        print(f"\n{what}: {len(new)} new violation(s), not on the allow-list:")
        print("\n".join(new))
    if stale:
        print(f"\n{what}: {len(stale)} allow-list entr(ies) no longer occur: remove them "
              "(`just boundaries tighten` does)")
        print("\n".join(stale))
    return new, stale


def check(edges_only):
    table = load_table()
    ws = Workspace(table, metadata())
    allow = table.get("allow") or {}
    bad = False

    edges = edge_violations(ws)
    new, stale = compare(edges, allow.get("edges", {}), "group rules")
    n_edges = sum(len(v) for v in edges.values())
    print(f"group rules: {len(ws.members)} members, {n_edges} edge(s) across a group line"
          f"{'' if new else ', all on the allow-list'}")
    bad |= bool(new or stale)

    if not edges_only:
        found, errors, n = promise_violations(ws)
        for name, err in sorted(errors.items()):
            print(f"  cargo tree -p {name}: {' '.join(err)}")
        new, stale = compare(found, allow.get("promise", {}), "promise")
        print(f"promise: {n} core/engine members resolved, {len(found)} reach a forbidden "
              f"package{'' if new else ', all on the allow-list'}")
        bad |= bool(new or stale or errors)

        rows = ws.table.get("deny", [])
        denied = deny_violations(ws)
        for name, hits in denied:
            why = next((r.get("why", "") for r in rows if r["package"] == name), "")
            print(f"\ndeny: {name} reaches {', '.join(hits)}{' (' + why + ')' if why else ''}")
        print(f"deny rows: {len(rows) - len(denied)} of {len(rows)} hold")
        bad |= bool(denied)

    if bad:
        print("\nThe rules and the allow-list: boundaries.toml. A new violation is fixed in the "
              "code (move the type to studio-core, or the UI to studio-connect), not allowed.")
    return 1 if bad else 0


def write_allow(edges, promise):
    text = TABLE.read_text()
    head = text.split(ALLOW_MARKER, 1)[0].rstrip() + "\n\n"
    out = [ALLOW_MARKER + " " + "─" * (76 - len(ALLOW_MARKER)),
           "# Written by `just boundaries tighten`, which only removes entries.",
           "# TODO(untangle): every entry below is a violation the untangling removes",
           "# (docs/MONOREPO-MIGRATION.md, the plan's \"What has to be untangled\").",
           "# Never add one: fix the dependency instead.",
           "",
           "# member -> the workspace crates it declares across a group line.",
           "[allow.edges]"]
    for pkg in sorted(edges):
        if edges[pkg]:
            out.append(f'"{pkg}" = [{", ".join(json.dumps(d) for d in sorted(edges[pkg]))}]')
    out += ["", "# member -> the forbidden categories its default `cargo tree -e normal` reaches.",
            "[allow.promise]"]
    for pkg in sorted(promise):
        if promise[pkg]:
            out.append(f'"{pkg}" = [{", ".join(json.dumps(c) for c in sorted(promise[pkg]))}]')
    TABLE.write_text(head + "\n".join(out) + "\n")


def tighten(seed):
    table = load_table()
    ws = Workspace(table, metadata())
    allow = table.get("allow") or {}
    edges = edge_violations(ws)
    promise, errors, _ = promise_violations(ws)
    if errors:
        die("cargo tree failed for: " + ", ".join(sorted(errors)))
    if seed:
        # The first run, or a reset the maintainers agreed to: today's state.
        keep_e = {p: set(d) for p, d in edges.items()}
        keep_p = {p: set(c) for p, c in promise.items()}
    else:
        keep_e = {p: set(d) & set(edges.get(p, ())) for p, d in allow.get("edges", {}).items()}
        keep_p = {p: set(c) & set(promise.get(p, ())) for p, c in allow.get("promise", {}).items()}
    write_allow(keep_e, keep_p)
    print(f"boundaries.toml: {sum(map(len, keep_e.values()))} edge(s), "
          f"{sum(1 for v in keep_p.values() if v)} promise entr(ies) allowed")
    return 0


def packages(groups, tested):
    table = load_table()
    ws = Workspace(table, metadata())
    if groups == ["all"]:
        groups = list(ws.groups)
    unknown = [g for g in groups if g not in ws.groups]
    if unknown:
        die(f"no group {', '.join(unknown)} (groups: {', '.join(ws.groups)}, all)")
    skip = set((table.get("test") or {}).get("skip", [])) if tested else set()
    print("\n".join(n for n in ws.in_group(*groups) if n not in skip))
    return 0


def main(argv):
    cmd = argv[0] if argv else "check"
    if cmd == "check":
        return check("--edges" in argv)
    if cmd == "tighten":
        return tighten("--seed" in argv)
    if cmd == "packages":
        return packages([a for a in argv[1:] if not a.startswith("--")] or ["all"], "--tested" in argv)
    if cmd == "groups":
        ws = Workspace(load_table(), metadata())
        for n, m in sorted(ws.members.items(), key=lambda kv: (kv[1]["group"], kv[0])):
            print(f"{m['group']:8} {n:32} {m['dir']}")
        return 0
    die(f"unknown command {cmd!r}: check [--edges] | tighten | packages GROUP... [--tested] | groups")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
