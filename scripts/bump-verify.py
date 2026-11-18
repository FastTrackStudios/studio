#!/usr/bin/env python3
"""Is <sha> on main the squash merge of a real bot pin bump? (release-queue.yml)

    python3 scripts/bump-verify.py <sha>                 # GitHub's API (GH_TOKEN)
    python3 scripts/bump-verify.py <sha> --fixtures DIR  # tests

`release.sh bump-queued` finds candidates from the commit's author and its
`Release-Queue: bump` trailer, which are free text: every agent PR is
authored by fts-forge[bot], and any PR body can carry the trailer. This
checks, through the API, that the commit is the merge of a pull request
  - merged into main, whose merge commit is <sha>;
  - from a branch `bump/architect-*` of THIS repository;
  - opened by fts-forge[bot] (the bump bot);
  - changing only what a bump may change (the bump workflow's allow-list).
Exit 0 and print the PR number, or exit 1 and say why.
"""
from __future__ import annotations

import json
import os
import re
import sys
import urllib.request
from pathlib import Path

REPO = os.environ.get("GITHUB_REPOSITORY", "FastTrackStudios/studio")
ALLOWED = re.compile(r"^(Cargo\.toml|.*/Cargo\.toml|Cargo\.lock|\.config/duplicate-deps\.toml|"
                     r"\.changes/studio-architect-on-architect-[0-9a-f]+\.md)$")


def get(path: str, fixtures: Path | None):
    if fixtures:
        f = fixtures / (path.replace("/", "_").replace("?", "_") + ".json")
        return json.loads(f.read_text()) if f.exists() else []
    req = urllib.request.Request(f"https://api.github.com/{path}", headers={"Accept": "application/vnd.github+json"})
    if os.environ.get("GH_TOKEN"):
        req.add_header("Authorization", f"Bearer {os.environ['GH_TOKEN']}")
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def main() -> int:
    args = sys.argv[1:]
    fixtures = None
    if "--fixtures" in args:
        i = args.index("--fixtures")
        fixtures = Path(args[i + 1])
        del args[i:i + 2]
    if len(args) != 1 or not re.fullmatch(r"[0-9a-f]{40}", args[0]):
        sys.exit("usage: bump-verify.py <sha> [--fixtures DIR]")
    sha = args[0]
    prs = [p for p in get(f"repos/{REPO}/commits/{sha}/pulls", fixtures)
           if p.get("merge_commit_sha") == sha and p.get("merged_at")]
    if len(prs) != 1:
        print(f"bump-verify: {sha} is not the merge of exactly one merged PR ({len(prs)} found)")
        return 1
    pr = prs[0]
    n = pr["number"]
    why = []
    if (pr.get("base") or {}).get("ref") != "main":
        why.append(f"merged into {pr.get('base', {}).get('ref')}, not main")
    head = pr.get("head") or {}
    if not str(head.get("ref", "")).startswith("bump/architect-"):
        why.append(f"from branch {head.get('ref')}, not bump/architect-*")
    if (head.get("repo") or {}).get("full_name") != REPO:
        why.append(f"from {(head.get('repo') or {}).get('full_name')}, not {REPO}")
    if (pr.get("user") or {}).get("login") != "fts-forge[bot]":
        why.append(f"opened by {(pr.get('user') or {}).get('login')}, not fts-forge[bot]")
    files, page = [], 1
    while True:
        batch = get(f"repos/{REPO}/pulls/{n}/files?per_page=100&page={page}", fixtures)
        files += [f["filename"] for f in batch]
        if len(batch) < 100 or fixtures:
            break
        page += 1
    if not files:
        why.append("changes no file")
    stray = [f for f in files if not ALLOWED.match(f)]
    if stray:
        why.append("changes files a bump may not: " + ", ".join(stray))
    if why:
        print(f"bump-verify: #{n} ({sha[:12]}) is not a bot pin bump: " + "; ".join(why))
        return 1
    print(n)
    return 0


if __name__ == "__main__":
    sys.exit(main())
