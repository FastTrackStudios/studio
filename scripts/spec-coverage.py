#!/usr/bin/env python3
"""Spec coverage: every requirement the specs define, and the code that carries it.

Ignition's check (its `features/lighting/tools/spec_coverage.py`, CI's `Spec
coverage`), widened to every spec in the monorepo. A requirement is an
`r[<id>]` paragraph opener in a spec file; code carries it with
`r[impl <id>]` / `r[verify <id>]`.

    python3 -I scripts/spec-coverage.py [--scope lighting] [--summary|--uncovered|--untested]

    --summary    (default) reqs / impl / verify per spec file
    --uncovered  ids no code implements
    --untested   ids no test verifies

`--scope lighting` is Ignition's check as its CI ran it (`just spec-coverage`,
CI's `Spec coverage`): its spec against its crates, failing (exit 1) on a tag
naming an id no spec defines, which is how a renamed or deleted rule is
caught. Without a scope it reads every spec and all the code and only
reports: tags whose specs are still in docs/imported/ (dsp, delay, rpp, ...)
show as unknown there until those specs move under docs/spec/.
"""
import collections
import glob
import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

# Where specs live. Ignition's lighting spec is still in its imported docs
# until it moves to docs/lighting/spec; the first folder that exists wins.
LIGHTING_SPEC = ["docs/lighting/spec/*.md", "docs/imported/Ignition/docs/lighting/spec/*.md"]
SCOPES = {
    None: {
        "specs": [["docs/spec/**/*.md"], LIGHTING_SPEC, ["crates/**/spec/**/*.md"]],
        "code": ["crates/**/*.rs", "apps/**/*.rs", "xtask/**/*.rs", "crates/**/tools/*.py", "crates/**/tools/*.sh"],
    },
    # Ignition's crates, wherever the monorepo put them.
    "lighting": {
        "specs": [LIGHTING_SPEC],
        "code": ["crates/studio-engine/lighting/**/*.rs", "crates/studio-engine/lighting/tools/*.py",
                 "crates/studio-engine/lighting/tools/*.sh", "crates/studio-connect/views/lighting/**/*.rs",
                 "crates/studio-core/proto/ignition-*/**/*.rs", "crates/studio-core/session/ignition-session/**/*.rs"],
    },
}

RULE = re.compile(r"^r\[([a-z0-9][a-z0-9._-]*)(?:@[^\]]*)?\]\s*$", re.M)
TAG = re.compile(r"r\[(impl|verify) ([a-z0-9][a-z0-9._-]*)(?:@[^\]]*)?\]")


def spec_files(scope):
    out = []
    for choices in SCOPES[scope]["specs"]:
        for pat in choices:
            found = glob.glob(os.path.join(ROOT, pat), recursive=True)
            if found:
                out += found
                break
    return sorted(f for f in set(out) if "/target/" not in f)


def main(mode, scope):
    spec = {}
    for f in spec_files(scope):
        with open(f, encoding="utf-8") as fh:
            for m in RULE.finditer(fh.read()):
                spec[m.group(1)] = os.path.relpath(f, ROOT)
    impl, verify, unknown = collections.defaultdict(list), collections.defaultdict(list), []
    for pat in SCOPES[scope]["code"]:
        for f in glob.glob(os.path.join(ROOT, pat), recursive=True):
            if "/target/" in f:
                continue
            rel = os.path.relpath(f, ROOT)
            try:
                with open(f, encoding="utf-8", errors="replace") as fh:
                    text = fh.read()
            except FileNotFoundError:
                continue  # removed while this ran
            for m in TAG.finditer(text):
                (impl if m.group(1) == "impl" else verify)[m.group(2)].append(rel)
                if m.group(2) not in spec:
                    unknown.append((rel, m.group(2)))

    if mode == "--uncovered":
        print("\n".join(i for i in sorted(spec) if i not in impl))
    elif mode == "--untested":
        print("\n".join(i for i in sorted(spec) if i not in verify))
    else:
        by = collections.defaultdict(lambda: [0, 0, 0])
        for i, f in spec.items():
            t = by[f]
            t[0] += 1
            t[1] += i in impl
            t[2] += i in verify
        w = max([len(f) for f in by] + [5])
        print(f"{'spec':{w}} {'reqs':>5} {'impl':>5} {'verify':>6}")
        for f, (n, a, b) in sorted(by.items()):
            print(f"{f:{w}} {n:5} {a:5} {b:6}")
        print(f"{'total':{w}} {len(spec):5} {sum(i in impl for i in spec):5} "
              f"{sum(i in verify for i in spec):6}")
    if unknown:
        print(f"\n{len(unknown)} tag(s) name an id no spec defines:")
        for rel, i in sorted(set(unknown)):
            print(f"  {rel}: {i}")
        return 1 if scope else 0
    return 0


if __name__ == "__main__":
    args = sys.argv[1:]
    scope = None
    if "--scope" in args:
        at = args.index("--scope")
        scope = args[at + 1] if at + 1 < len(args) else ""
        del args[at:at + 2]
        if scope not in SCOPES:
            sys.exit(f"spec-coverage: no scope {scope!r} (lighting)")
    sys.exit(main(args[0] if args else "--summary", scope))
