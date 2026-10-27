#!/usr/bin/env python3
"""The engine boundary inside session-daw: no engine module reaches a view module.

Studio's engine and its window are being split (docs/design/engine-boundary.md):
the modules listed in apps/session/daw/engine-modules.txt are the engine side,
everything else in apps/session/daw/src is the window's. An engine module may
use another engine module or any crate; it may not name a window module
(`crate::widget::…`, `use crate::{shell, …}`, `super::super::panel` from a
nested file) — in production code or in its tests. A view module may use the
engine freely.

Until the crate cut (`session-engine` / `session-ui`) makes the compiler say
so, this says so: `just engine-boundary`, run by `just ci-local`.

    python3 scripts/engine-boundary.py            # exit 1 on any crossing
    python3 scripts/engine-boundary.py --list     # every module, by side
"""
import os, re, sys
from collections import defaultdict

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "apps/session/daw/src")
LIST = os.path.join(ROOT, "apps/session/daw/engine-modules.txt")

def strip(src):
    out = []; i = 0; n = len(src)
    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i); j = n if j < 0 else j
            out.append(" " * (j - i)); i = j; continue
        if src.startswith("/*", i):
            depth = 1; j = i + 2
            while j < n and depth:
                if src.startswith("/*", j): depth += 1; j += 2
                elif src.startswith("*/", j): depth -= 1; j += 2
                else: j += 1
            out.append(re.sub(r"[^\n]", " ", src[i:j])); i = j; continue
        m = re.match(r'b?r(#*)"', src[i:i+10])
        if m and (i == 0 or not (src[i-1].isalnum() or src[i-1] == "_")):
            hashes = m.group(1); end = '"' + hashes
            j = src.find(end, i + len(m.group(0)))
            j = n if j < 0 else j + len(end)
            out.append(re.sub(r"[^\n]", " ", src[i:j])); i = j; continue
        if c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            j += 1
            out.append(re.sub(r"[^\n]", " ", src[i:j])); i = j; continue
        if c == "'":
            # char literal vs lifetime
            m = re.match(r"'(\\.[^']*|[^\\'])'", src[i:i+12])
            if m:
                out.append(" " * len(m.group(0))); i += len(m.group(0)); continue
        out.append(c); i += 1
    return "".join(out)

def test_ranges(s):
    ranges = []
    for m in re.finditer(r"#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{", s):
        start = m.end() - 1; depth = 0; j = start
        while j < len(s):
            if s[j] == "{": depth += 1
            elif s[j] == "}":
                depth -= 1
                if depth == 0: break
            j += 1
        ranges.append((m.start(), j))
    return ranges

def split_top(group):
    parts = []; depth = 0; cur = ""
    for ch in group:
        if ch == "{": depth += 1
        elif ch == "}": depth -= 1
        if ch == "," and depth == 0:
            parts.append(cur); cur = ""
        else:
            cur += ch
    parts.append(cur)
    return [p.strip() for p in parts if p.strip()]

def match_brace(s, i):
    depth = 0
    for j in range(i, len(s)):
        if s[j] == "{": depth += 1
        elif s[j] == "}":
            depth -= 1
            if depth == 0: return j
    return len(s)

def names_in_group_item(item):
    """item like 'arrangement::{A, B}' or 'widget' or 'tone::Foo as Bar' -> (mod, [syms], alias_mod)"""
    m = re.match(r"(\w+)(.*)", item, re.S)
    if not m: return None
    mod, rest = m.group(1), m.group(2).strip()
    syms = []
    alias_for_mod = None
    if rest.startswith("::"):
        rest = rest[2:].strip()
        if rest.startswith("{"):
            inner = rest[1:match_brace(rest, 0)]
            for it in split_top(inner):
                s = re.match(r"(\w+|\*)", it)
                if s: syms.append(s.group(1))
        else:
            s = re.match(r"(\w+|\*)", rest)
            if s: syms.append(s.group(1))
    else:
        a = re.match(r"as\s+(\w+)", rest)
        alias_for_mod = a.group(1) if a else mod
        syms.append("(module)")
    return mod, syms, alias_for_mod


def module_deps():
    """{module: {target: set(symbols)}} over src, tests included."""
    modules = {}
    for f in sorted(os.listdir(SRC)):
        p = os.path.join(SRC, f)
        if f.endswith(".rs") and f != "lib.rs":
            modules[f[:-3]] = [(p, 0)]
        elif os.path.isdir(p) and f != "bin":
            files = []
            for root, _, fs in os.walk(p):
                for x in fs:
                    if x.endswith(".rs"):
                        q = os.path.join(root, x)
                        depth = os.path.relpath(q, p).count(os.sep) + (0 if x == "mod.rs" else 1)
                        files.append((q, depth))
            modules[f] = sorted(files)
    modset = set(modules)
    # lib.rs `pub use` re-exports at crate root: none expected, check

    deps = defaultdict(lambda: defaultdict(lambda: {"prod": set(), "test": set()}))
    lines = {}
    for mod, files in modules.items():
        total = 0
        for path, depth in files:
            raw = open(path).read(); total += raw.count("\n")
            s = strip(raw)
            whole_test = os.path.basename(path) == "tests.rs" or mod == "hook_rules"
            tr = test_ranges(s)
            def kind(pos):
                if whole_test: return "test"
                return "test" if any(a <= pos <= b for a, b in tr) else "prod"
            def add(target, sym, pos):
                if target in modset and target != mod:
                    deps[mod][target][kind(pos)].add(sym)
            imported = {}  # local alias -> module
            # use crate::{...} groups
            for m in re.finditer(r"\bcrate\s*::\s*\{", s):
                b = m.end() - 1; e = match_brace(s, b)
                for it in split_top(s[b+1:e]):
                    r = names_in_group_item(it)
                    if not r: continue
                    tgt, syms, alias = r
                    for sy in syms: add(tgt, sy, m.start())
                    if alias and tgt in modset: imported[alias] = tgt
            # crate::X::... and use crate::X;
            for m in re.finditer(r"\bcrate\s*::\s*(\w+)(\s*::\s*(\{|\w+|\*))?(\s+as\s+(\w+))?", s):
                tgt = m.group(1)
                if m.group(3) == "{":
                    b = m.end(3) - 1; e = match_brace(s, b)
                    for it in split_top(s[b+1:e]):
                        x = re.match(r"(\w+|\*)", it)
                        if x: add(tgt, x.group(1), m.start())
                elif m.group(3):
                    add(tgt, m.group(3), m.start())
                else:
                    add(tgt, "(module)", m.start())
                    # `use crate::X;` / `use crate::X as Y;`
                    pre = s[max(0, m.start()-40):m.start()]
                    if re.search(r"\buse\s+$", pre) and tgt in modset:
                        imported[m.group(5) or tgt] = tgt
            # super::super::X from nested files
            if depth >= 1:
                pat = r"\b" + r"super\s*::\s*" * (depth + 1) + r"(\w+)(?:\s*::\s*(\w+|\{))?"
                for m in re.finditer(pat, s):
                    add(m.group(1), m.group(2) or "(module)", m.start())
            # bare alias::sym paths for imported module aliases
            for alias, tgt in imported.items():
                for m in re.finditer(r"(?<![\w:])" + re.escape(alias) + r"\s*::\s*(\w+)", s):
                    add(tgt, m.group(1), m.start())
        lines[mod] = total
    return modset, {m: {t: d["prod"] | d["test"] for t, d in ts.items()} for m, ts in deps.items()}


def engine_modules():
    with open(LIST) as f:
        return {l.split("#")[0].strip() for l in f if l.split("#")[0].strip()}


def main():
    modset, deps = module_deps()
    engine = engine_modules()
    unknown = sorted(engine - modset)
    if "--list" in sys.argv:
        print("engine:", " ".join(sorted(engine & modset)))
        print("view:  ", " ".join(sorted(modset - engine)))
    crossings = []
    for mod in sorted(engine & modset):
        for target, syms in sorted(deps.get(mod, {}).items()):
            if target not in engine and syms:
                crossings.append((mod, target, sorted(syms)))
    for mod, target, syms in crossings:
        print(f"engine module `{mod}` uses view module `{target}`: {', '.join(syms)}")
    for mod in unknown:
        print(f"engine-modules.txt names `{mod}`, which is not a module of session-daw")
    if crossings or unknown:
        print(f"engine boundary: {len(crossings)} crossing(s) — move the symbol to the engine side, "
              "or invert the call (a hook the window registers, a change it listens for)")
        sys.exit(1)
    print(f"engine boundary: {len(engine & modset)} engine modules, none reaches a view module")


if __name__ == "__main__":
    main()
