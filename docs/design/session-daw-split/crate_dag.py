import json, re, os
from collections import defaultdict
from proposal import ASSIGN, ORDER, edges, home, deps
rank={c:i for i,c in enumerate(ORDER)}
G=defaultdict(set)
for (a,b),v in edges.items(): G[a].add(b)
def reach(a,skip):
    seen=set(); st=[x for x in G[a] if (a,x)!=skip]
    while st:
        x=st.pop()
        if x in seen: continue
        seen.add(x); st+=list(G[x])
    return seen
red={a:sorted([b for b in G[a] if b not in reach(a,(a,b))], key=lambda c:rank[c]) for a in ORDER}
full={a:sorted(G[a], key=lambda c:rank[c]) for a in ORDER}
# pub(crate) symbols that cross crates
SRC="/run/media/Development/fts/studio/apps/session/daw/src"
def src_of(m):
    p=f"{SRC}/{m}.rs"
    if os.path.exists(p): return open(p).read()
    return "".join(open(os.path.join(r,f)).read() for r,_,fs in os.walk(f"{SRC}/{m}") for f in fs if f.endswith(".rs"))
cross=set()
for (a,b),v in edges.items():
    for m,t,s in v: cross.add((t,s))
pc=sorted(f"{t}::{s}" for t,s in cross if re.search(r"pub\(crate\)\s+(?:const\s+)?(?:async\s+)?(?:fn|struct|enum|const|static|type|trait|mod)\s+"+re.escape(s)+r"\b", src_of(t)))
json.dump({"direct":full,"transitive_reduction":red,"pub_crate_to_widen":pc},open("crate_dag.json","w"),indent=1)
for a in ORDER: print(f"{a:12} -> {red[a]}")
print(len(pc),"pub(crate) items would need `pub`")
