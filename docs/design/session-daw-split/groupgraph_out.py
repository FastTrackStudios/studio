"""Deliverable 2+3 at the ORIGINAL (agreed) grouping: group edges with refs, SCCs, and mutual pairs."""
import json
from collections import defaultdict
from groups import GROUP
D = json.load(open("module_deps_detailed.json"))["deps"]
E = defaultdict(list)
for m, ts in D.items():
    for t, k in ts.items():
        if k.get("prod") and GROUP[m] != GROUP[t]:
            E[(GROUP[m], GROUP[t])].append(f"{m} -> {t}: {', '.join(k['prod'])}")
json.dump({f"{a} -> {b}": {"module_edges": len(v), "symbols": sum(x.count(',') + 1 for x in v), "refs": v}
           for (a, b), v in sorted(E.items())}, open("group_graph.json", "w"), indent=1)
G = defaultdict(set)
for a, b in E: G[a].add(b)
# SCC
idx, low, st, on, out, c = {}, {}, [], set(), [], [0]
def f(v):
    idx[v] = low[v] = c[0]; c[0] += 1; st.append(v); on.add(v)
    for w in G[v]:
        if w not in idx: f(w); low[v] = min(low[v], low[w])
        elif w in on: low[v] = min(low[v], idx[w])
    if low[v] == idx[v]:
        comp = []
        while True:
            w = st.pop(); on.discard(w); comp.append(w)
            if w == v: break
        out.append(comp)
for v in list(G): 
    if v not in idx: f(v)
with open("group_cycles.txt", "w") as fh:
    fh.write("SCCs over the agreed groups (prod refs only):\n")
    for s in out: fh.write(f"  {sorted(s)}\n")
    fh.write("\nMutual pairs (2-cycles); the lighter direction is the natural cut:\n")
    seen = set()
    for (a, b), v in sorted(E.items()):
        if (b, a) in E and (b, a) not in seen:
            seen.add((a, b)); w = E[(b, a)]
            light, heavy = ((a, b), v), ((b, a), w)
            if len(w) < len(v): light, heavy = heavy, light
            fh.write(f"\n{a} <-> {b}: {a}->{b} {len(v)} module-edges, {b}->{a} {len(w)}\n  cut {light[0][0]} -> {light[0][1]}:\n")
            for r in light[1]: fh.write(f"    {r}\n")
print(open("group_cycles.txt").read()[:400])
