import json, re, os
from collections import defaultdict
from proposal import ASSIGN, EXTRACT, INVERT, ORDER, home, deps
rank = {c: i for i, c in enumerate(ORDER)}
print("== extracted symbols whose SOURCE module has deps above the target crate (check the symbol does not drag them):")
bymod = defaultdict(lambda: defaultdict(list))
for (m, s), c in EXTRACT.items(): bymod[m][c].append(s)
for m, cs in sorted(bymod.items()):
    for c, syms in cs.items():
        above = sorted({ASSIGN[t] for t, k in deps[m].items() if k.get("prod") and rank[ASSIGN[t]] > rank[c]})
        if above: print(f"  {m}::{{{', '.join(syms)}}} -> {c}; {m} also uses {above}")
print("== test-only references that would go upward (need dev-deps / moving the test):")
for m, ts in deps.items():
    for t, k in ts.items():
        for s in k.get("test", []):
            if s in k.get("prod", []): continue
            a, b = ASSIGN[m], home(t, s)
            if a != b and rank[a] < rank[b]: print(f"  {m} -> {t}::{s}  ({a} -> {b})")
ROOT = "/run/media/Development/fts/studio/apps/session/daw"
print("== bins / integration tests -> crates used:")
for d in ["src/bin", "tests"]:
    p = os.path.join(ROOT, d)
    if not os.path.isdir(p): continue
    for f in sorted(os.listdir(p)):
        if not f.endswith(".rs"): continue
        src = open(os.path.join(p, f)).read()
        mods = set(re.findall(r"session_daw::(\w+)", src))
        for g in re.findall(r"session_daw::\{([^}]*)\}", src):
            mods |= set(re.findall(r"\b(\w+)\b", g))
        crates = sorted({ASSIGN[x] for x in mods if x in ASSIGN}, key=lambda c: rank[c])
        print(f"  {d}/{f}: {crates}")
