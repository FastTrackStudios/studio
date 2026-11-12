#!/usr/bin/env python3
"""Flatten a wasm-bindgen `--target web` folder for Architect's web view
(`SiteIsland::web`): every module a plain `web-*` name of one folder, the
imports between them `./<name>.js`. Its snippets (snippets/<crate>/…js)
become web-snippet-<n>.js; nothing else changes.

    island-web-flatten.py <wasm-bindgen out dir> <island folder>
"""
import os, re, shutil, sys

src, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
modules = []
for root, _, files in os.walk(src):
    for f in files:
        modules.append(os.path.relpath(os.path.join(root, f), src))
snippets = sorted(m for m in modules if m.startswith("snippets/") and m.endswith(".js"))
name = {m: f"web-snippet-{i}.js" for i, m in enumerate(snippets)}
for m in modules:
    if not m.startswith("snippets/"):
        name[m] = m
spec = re.compile(r"""(\bfrom\s*|\bimport\s*\(\s*|\bimport\s+)(["'])(\.{1,2}/[^"']+)\2""")
for m in modules:
    path = os.path.join(src, m)
    if not m.endswith(".js"):
        shutil.copyfile(path, os.path.join(out, name[m]))
        continue
    text = open(path, encoding="utf-8").read()
    def fix(match):
        target = os.path.normpath(os.path.join(os.path.dirname(m), match.group(3)))
        if target not in name:
            sys.exit(f"{m} imports {match.group(3)}, which is not in the bundle")
        return f"{match.group(1)}{match.group(2)}./{name[target]}{match.group(2)}"
    text = spec.sub(fix, text)
    open(os.path.join(out, name[m]), "w", encoding="utf-8").write(text)
for n in name.values():
    if not re.fullmatch(r"web-[A-Za-z0-9._-]+\.(js|wasm)", n):
        sys.exit(f"{n} is not a web view file name (web-*.js, web-*.wasm)")
print("\n".join(sorted(name.values())))
