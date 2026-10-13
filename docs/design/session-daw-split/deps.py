#!/usr/bin/env python3
"""Module dependency extraction for apps/session/daw (session-daw).

For every top-level module M (src/M.rs or src/M/**.rs) find references to other
top-level modules, via:
  * `crate::X...` paths anywhere (incl. `use crate::X::...`)
  * `use crate::{X, Y::{a,b}, ...}` groups (top-level entries of the brace group)
  * `super::super::X` from files inside a directory module (src/D/f.rs)
  * bare `X::sym` where X was brought into scope by a `use crate::...X` import
`super::` in top-level files is ignored (verified: only used by inline test mods).
Comments (incl. doc intra-links) and string literals are stripped first.
References inside `#[cfg(test)] mod … { }` blocks, or in tests.rs files / hook_rules
(cfg(test) at lib level), are recorded separately as test-only.
Records the symbol used (first segment after the module) for each edge.
"""
import json, os, re, sys
from collections import defaultdict

SRC = "/run/media/Development/fts/studio/apps/session/daw/src"

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

def main():
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
    lib = strip(open(os.path.join(SRC, "lib.rs")).read())
    reexports = re.findall(r"pub use crate::(\w+)", lib)

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
    out = {}
    for mod in sorted(modset):
        out[mod] = {t: {k: sorted(v) for k, v in d.items() if v} for t, d in sorted(deps[mod].items())}
    json.dump({"reexports_in_lib": reexports, "lines": lines, "deps": out},
              open("module_deps_detailed.json", "w"), indent=1)
    simple = {m: sorted(t for t, d in out[m].items() if d.get("prod")) for m in out}
    json.dump(simple, open("module_deps.json", "w"), indent=1)
    simple_t = {m: sorted(t for t in out[m]) for m in out}
    json.dump(simple_t, open("module_deps_incl_tests.json", "w"), indent=1)
    print(len(modset), "modules;", sum(len(v) for v in simple.values()), "prod edges; reexports:", reexports)

if __name__ == '__main__':
    main()
