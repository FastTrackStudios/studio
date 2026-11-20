#!/usr/bin/env python3
"""The local stack's file work (scripts/local-stack.sh calls it): reading
local-stack.toml and Cargo.lock, writing the [patch] overrides that put a
local Architect or Keyflow under this checkout's build, and seeding the
node's copy of the test vault with its share link.

    local-stack.py get KEY                       a local-stack.toml value (a.b)
    local-stack.py pinned URL CRATE              "<tag> <rev>" Cargo.lock pins CRATE at
    local-stack.py patch URL CHECKOUT            [patch."URL"] for every locked crate of URL
    local-stack.py dep-tag MANIFEST URL          the tag MANIFEST's deps of URL name
    local-stack.py seed-share VAULT SETLIST CODE VAULT_ID   the share note, written
    local-stack.py seed DST VAULT_ID      the demo vault (made when its sources changed) and every share
"""

import json
import contextlib
import io
import shutil
import subprocess
import os
import re
import sys
import tomllib
from typing import NoReturn

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def die(msg) -> NoReturn:
    sys.exit(f"local-stack: {msg}")


def locked(url):
    """Every package of Cargo.lock whose source is the git repo URL."""
    with open(os.path.join(ROOT, "Cargo.lock"), "rb") as f:
        lock = tomllib.load(f)
    # Either spelling of the host: since Studio pins Architect by tag
    # (studio#539) the root [patch] hands every Architect crate over from
    # `www.github.com`, and that is the source the lock names.
    prefixes = {f"git+{url}", f"git+{url.replace('https://github.com/', 'https://www.github.com/')}"}
    return [p for p in lock.get("package", []) if p.get("source", "").split("?")[0].split("#")[0] in prefixes]


def get(key):
    # The vault's setlist and share code can be named for one run
    # (LOCAL_STACK_SETLIST, LOCAL_STACK_SHARE_CODE), as LOCAL_STACK_VAULT
    # names another vault: a copy of a real set without editing the file.
    override = {"vault.setlist": "LOCAL_STACK_SETLIST", "vault.share_code": "LOCAL_STACK_SHARE_CODE"}.get(key)
    if override and os.environ.get(override):
        print(os.environ[override])
        return
    with open(os.path.join(ROOT, "local-stack.toml"), "rb") as f:
        value = tomllib.load(f)
    for part in key.split("."):
        value = value[part]
    print(value)


def pinned(url, crate):
    for p in locked(url):
        if p["name"] == crate:
            src = p["source"]
            tag = re.search(r"[?&]tag=([^#&]+)", src)
            rev = src.rsplit("#", 1)[-1]
            print(f"{tag.group(1) if tag else '-'} {rev}")
            return
    die(f"Cargo.lock has no {crate} from {url}")


def manifests(checkout):
    """name -> dir of every package manifest in CHECKOUT (not target/, .git, node_modules)."""
    out = {}
    skip = {"target", ".git", "node_modules", ".devenv", ".direnv"}
    for d, dirs, files in os.walk(checkout):
        dirs[:] = [x for x in dirs if x not in skip and not x.startswith("target")]
        if "Cargo.toml" not in files:
            continue
        try:
            with open(os.path.join(d, "Cargo.toml"), "rb") as f:
                m = tomllib.load(f)
        except (OSError, tomllib.TOMLDecodeError):
            continue
        name = m.get("package", {}).get("name")
        if isinstance(name, str):
            out.setdefault(name, d)
    return out


def patch(url, checkout):
    """The [patch] table that replaces every crate this checkout's lock takes
    from URL with the same crate in CHECKOUT. A crate the checkout no longer
    has is named on stderr and left as locked (cargo then says why)."""
    have = manifests(checkout)
    names = sorted({p["name"] for p in locked(url)})
    if not names:
        die(f"Cargo.lock takes nothing from {url}")
    print(f'[patch."{url}"]')
    for name in names:
        if name in have:
            print(f'{name} = {{ path = {json.dumps(have[name])} }}')
        else:
            print(f"local-stack: {checkout} has no crate {name} (locked from {url}): it stays as locked", file=sys.stderr)


def dep_tag(manifest, url):
    with open(manifest, "rb") as f:
        m = tomllib.load(f)
    tags = set()
    for table in (m.get("workspace", {}).get("dependencies", {}), m.get("dependencies", {})):
        for dep in table.values():
            if isinstance(dep, dict) and dep.get("git", "").rstrip("/") == url:
                tags.add(dep.get("tag") or dep.get("rev") or dep.get("branch") or "?")
    print(" ".join(sorted(tags)))


LINK = re.compile(r"\[\[([^|\]]+)")


def frontmatter(text):
    if not text.startswith("---\n"):
        return ""
    end = text.find("\n---", 4)
    return text[4:end] if end > 0 else ""


def slug_segment(name):
    """architect-sites `url::slug_segment`: letters, digits, `-_.` kept
    (lowercased), a run of spaces or `+` one `-`, anything else dropped."""
    out = ""
    for ch in name.strip():
        if ch.isalnum() or ch in "-_.":
            out += ch.lower()
        elif (ch.isspace() or ch == "+") and out and not out.endswith("-"):
            out += "-"
    return out.rstrip("-")


def seed_share(vault, setlist, code, vault_id, listen="Reference"):
    if listen not in ("Reference", "Full"):
        die(f"listen `{listen}` for {setlist}: Reference or Full")
    """The share note SHARE_CREATE writes for STUDIO_SETLISTS_SHARE: the
    setlist's note and each of its songs' notes, for anyone with the link
    (apps/desktop/tests/node.rs builds the same)."""
    if not re.fullmatch(r"[abcdefghijkmnpqrstuvwxyz23456789]{16}", code):
        die(f"share_code `{code}` is not 16 characters of abcdefghijkmnpqrstuvwxyz23456789")
    path = os.path.join(vault, setlist)
    try:
        text = open(path, encoding="utf-8").read()
    except OSError as e:
        die(f"the setlist {setlist} is not in {vault}: {e}")
    fm = frontmatter(text)
    # YAML's quotes off (a single-quoted scalar doubles its apostrophes: 'JT''s').
    title = next((l.split(":", 1)[1].strip() for l in fm.splitlines() if l.startswith("title:")), None)
    if title and len(title) > 1 and title[0] == title[-1] == "'":
        title = title[1:-1].replace("''", "'")
    elif title:
        title = title.strip('"')
    title = title or os.path.splitext(os.path.basename(setlist))[0]
    # The top-level `songs:` list (each set lists them again).
    songs, inside = [], False
    for line in fm.splitlines():
        if re.match(r"^songs:\s*$", line):
            inside = True
            continue
        if inside:
            if line.startswith("- "):
                m = LINK.search(line)
                if m:
                    # A link to one arrangement of a song (`Song#Live`) shares the song's note.
                    note = m.group(1).replace("''", "'").split("#", 1)[0]
                    note = note if note.endswith(".md") else note + ".md"
                    if note not in songs:
                        songs.append(note)
            elif line and not line.startswith(" "):
                inside = False
    missing = [s for s in songs if not os.path.isfile(os.path.join(vault, s))]
    if missing:
        die(f"the setlist names songs the vault does not have: {missing}")
    share = {
        "code": code,
        "vault": vault_id,
        "scope": {"Notes": [setlist] + songs},
        "title": title,
        "audience": "Link",
        "access": "Read",
        "expires_at": None,
        "revoked_at": None,
        "created_at": 1794355200,
        "created_by": "local-stack",
        "domain": None,
        "audiences": [],
        # How far the link plays: Reference (the stack's default, a
        # listener's link) or Full (the multitracks too; `LOCAL_STACK_LISTEN`).
        "listen": listen,
    }
    note = (
        "---\npublish: false\n---\n"
        f"# Share {code}\n\n"
        f"A share link, kept by Architect (`SHARE_*`): the setlist {title} and its songs, for anyone with the link. "
        "Written by Studio's local stack (scripts/local-stack.sh).\n\n"
        f"```json\n{json.dumps(share, ensure_ascii=False, separators=(',', ':'))}\n```\n"
    )
    out = os.path.join(vault, "_architect", "shares", f"{code}.md")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", encoding="utf-8") as f:
        f.write(note)
    # The page path of the setlist: what the node's site builder makes of
    # its vault path (architect-sites `url::slug_segment`, per segment).
    slug = "/".join(slug_segment(part) for part in setlist[:-3].split("/"))
    print(f"{len(songs)} {slug}")


def stack_toml():
    with open(os.path.join(ROOT, "local-stack.toml"), "rb") as f:
        return tomllib.load(f)


def locate(path):
    """A path from local-stack.toml: absolute, else from this checkout,
    else from the main checkout of a worktree (scripts/local-stack.sh
    `locate`)."""
    if os.path.isabs(path):
        return path if os.path.exists(path) else None
    bases = [ROOT]
    try:
        common = subprocess.run(["git", "-C", ROOT, "rev-parse", "--path-format=absolute", "--git-common-dir"],
                                capture_output=True, text=True, check=True).stdout.strip()
        bases.append(os.path.dirname(common))
    except (OSError, subprocess.CalledProcessError):
        pass
    for base in bases:
        full = os.path.normpath(os.path.join(base, path))
        if os.path.exists(full):
            return full
    return None


def default_listen():
    """How far a link plays when its `[[vault.shares]]` entry does not say:
    `LOCAL_STACK_LISTEN`, else Reference."""
    listen = os.environ.get("LOCAL_STACK_LISTEN", "Reference")
    if listen not in ("Reference", "Full"):
        die(f"LOCAL_STACK_LISTEN=`{listen}`: Reference or Full")
    return listen


def seed(dst, vault_id):
    """The demo vault at DST: its sources merged into a copy (reflinked
    where the file system can), made again when the list of sources
    changed; then a share note per set. Prints `<code> <page>` per share.
    With no source found, prints the demo seed's link and copies nothing."""
    cfg = stack_toml().get("vault", {})
    one = os.environ.get("LOCAL_STACK_VAULT")
    wanted = [one] if one else cfg.get("sources", [])
    sources = [locate(s) for s in wanted]
    missing = [w for w, s in zip(wanted, sources) if not s]
    sources = [s for s in sources if s]
    if missing:
        print(f"local-stack: NOTICE: no vault at {', '.join(missing)} (from {ROOT})", file=sys.stderr)
    if not sources:
        print("local-stack: NOTICE: no demo vault source: the server holds Architect's demo seed (setlist Fall Show)", file=sys.stderr)
        print("ksqx75iaaaaaaaae setlists/fall-show")
        return
    stamp = "\n".join(sources) + "\n"
    stamp_file = os.path.join(dst, ".local-stack-sources")
    have = open(stamp_file, encoding="utf-8").read() if os.path.isfile(stamp_file) else None
    if have != stamp:
        if os.path.isdir(dst):
            print(f"local-stack: the vault's sources changed: making {dst} again", file=sys.stderr)
            shutil.rmtree(dst)
        tmp = dst + ".tmp"
        shutil.rmtree(tmp, ignore_errors=True)
        os.makedirs(tmp, exist_ok=True)
        for src in sources:
            print(f"local-stack: copying {src} into the demo vault", file=sys.stderr)
            subprocess.run(["cp", "-a", "--reflink=auto", src + "/.", tmp + "/"], check=True)
        with open(os.path.join(tmp, ".local-stack-sources"), "w", encoding="utf-8") as f:
            f.write(stamp)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        os.rename(tmp, dst)
    if os.environ.get("LOCAL_STACK_SETLIST"):
        shares = [{"setlist": os.environ["LOCAL_STACK_SETLIST"],
                   "code": os.environ.get("LOCAL_STACK_SHARE_CODE", "merrybrightstack"),
                   "listen": default_listen()}]
    else:
        shares = cfg.get("shares", [])
    for share in shares:
        if not os.path.isfile(os.path.join(dst, share["setlist"])):
            print(f"local-stack: NOTICE: the demo vault has no {share['setlist']}: no link for it", file=sys.stderr)
            continue
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            seed_share(dst, share["setlist"], share["code"], vault_id, share.get("listen", default_listen()))
        print(f"{share['code']} {out.getvalue().split(' ', 1)[1].strip()}")


def main(argv):
    if len(argv) < 2:
        die(__doc__)
    cmd, args = argv[1], argv[2:]
    {
        "get": get,
        "pinned": pinned,
        "patch": patch,
        "dep-tag": dep_tag,
        "seed-share": seed_share,
        "seed": seed,
    }.get(cmd, lambda *_: die(__doc__))(*args)


if __name__ == "__main__":
    main(sys.argv)
