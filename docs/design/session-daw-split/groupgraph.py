import json, sys
from collections import defaultdict
sys.path.insert(0, ".")
from groups import GROUP
D = json.load(open("module_deps_detailed.json"))
deps, lines = D["deps"], D["lines"]
missing = [m for m in deps if m not in GROUP]; print("unassigned:", missing)
extra = [m for m in GROUP if m not in deps]; print("not modules:", extra)
def ggraph(include_test=False):
    E = defaultdict(list)
    for m, ts in deps.items():
        for t, k in ts.items():
            syms = k.get("prod", []) + (k.get("test", []) if include_test else [])
            if not syms: continue
            a, b = GROUP[m], GROUP[t]
            if a != b: E[(a, b)].append((m, t, syms))
    return E
E = ggraph()
glines = defaultdict(int)
for m, n in lines.items(): glines[GROUP[m]] += n
print({g: glines[g] for g in sorted(glines)})
for (a, b), refs in sorted(E.items()):
    print(f"{a} -> {b}: {len(refs)} module-edges, {sum(len(r[2]) for r in refs)} symbols")
