import json,sys
from groups import GROUP
D=json.load(open("module_deps_detailed.json"))["deps"]
src,dst=sys.argv[1],sys.argv[2]
for m,ts in sorted(D.items()):
  if GROUP[m]!=src and m!=src: continue
  for t,k in sorted(ts.items()):
    if (GROUP[t]==dst or t==dst) and k.get("prod") and GROUP[t]!=GROUP[m]:
      print(f"  {m} -> {t}: {', '.join(k['prod'])}")
