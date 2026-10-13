import re, os, json, tomllib
from collections import defaultdict
from proposal import ASSIGN, ORDER
ROOT="/run/media/Development/fts/studio/apps/session/daw"
cargo=tomllib.load(open(ROOT+"/Cargo.toml","rb"))
names=set(cargo["dependencies"])|set(cargo.get("target",{}).get("cfg(target_arch = \"wasm32\")",{}).get("dependencies",{}))
alias={n:n.replace("-","_") for n in names}
alias["wgpu-context-07"]="wgpu_context_07"
from deps import strip  # reuse
SRC=ROOT+"/src"
per=defaultdict(set); permod=defaultdict(set)
for m,c in ASSIGN.items():
    files=[SRC+f"/{m}.rs"] if os.path.exists(SRC+f"/{m}.rs") else [os.path.join(r,f) for r,_,fs in os.walk(SRC+"/"+m) for f in fs if f.endswith(".rs")]
    for f in files:
        s=strip(open(f).read())
        for n,a in alias.items():
            if re.search(r"(?<![\w:])"+re.escape(a)+r"\s*::", s) or re.search(r"\buse\s+"+re.escape(a)+r"\b", s):
                per[c].add(n); permod[m].add(n)
out={c:sorted(per[c]) for c in ORDER}
json.dump({"per_crate":out,"per_module":{m:sorted(v) for m,v in sorted(permod.items())}},open("external_deps.json","w"),indent=1)
for c in ORDER: print(c, ":", " ".join(out[c]))
print("unused by any module:", sorted(names-set().union(*per.values())))
