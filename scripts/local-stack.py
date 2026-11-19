#!/usr/bin/env python3
"""The local stack's file work (scripts/local-stack.sh calls it): reading
local-stack.toml and Cargo.lock, writing the [patch] overrides that put a
local Architect or Keyflow under this checkout's build, and seeding the
node's copy of the test vault with its share link.

    local-stack.py get KEY [DEFAULT]             a setting (a.b): local-stack.local.toml over
                                                 local-stack.toml; DEFAULT (else an error) when neither has it
    local-stack.py pinned URL CRATE              "<tag> <rev>" Cargo.lock pins CRATE at
    local-stack.py patch URL CHECKOUT            [patch."URL"] for every locked crate of URL
    local-stack.py dep-tag MANIFEST URL          the tag MANIFEST's deps of URL name
    local-stack.py seed-share VAULT SETLIST CODE VAULT_ID   the share note, written
"""

import json
import os
import re
import sys
import tomllib
from typing import NoReturn

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def die(msg) -> NoReturn:
    sys.exit(f"local-stack: {msg}")


def lock_text():
    """The committed Cargo.lock (HEAD's): the working one may hold a patched
    build's path sources, which name no git source at all."""
    import subprocess
    try:
        return subprocess.run(["git", "-C", ROOT, "show", "HEAD:Cargo.lock"], check=True,
                              capture_output=True, text=True).stdout
    except (OSError, subprocess.CalledProcessError):
        with open(os.path.join(ROOT, "Cargo.lock"), encoding="utf-8") as f:
            return f.read()


def locked(url):
    """Every package whose source is the git repo URL: in the committed
    Cargo.lock, else in the working one."""
    prefix = f"git+{url}"
    out, seen = [], set()
    texts = [lock_text()]
    try:
        with open(os.path.join(ROOT, "Cargo.lock"), encoding="utf-8") as f:
            texts.append(f.read())
    except OSError:
        pass
    for text in texts:
        for p in tomllib.loads(text).get("package", []):
            if p.get("source", "").split("?")[0].split("#")[0] == prefix and p["name"] not in seen:
                seen.add(p["name"])
                out.append(p)
    return out


def declared(url):
    """The crates the workspace's Cargo.toml takes from URL (one not built
    yet is in no lock)."""
    with open(os.path.join(ROOT, "Cargo.toml"), "rb") as f:
        m = tomllib.load(f)
    deps = m.get("workspace", {}).get("dependencies", {})
    return {d.get("package", n) for n, d in deps.items() if isinstance(d, dict) and d.get("git", "").rstrip("/") == url}


def local_file():
    """This machine's settings: `LOCAL_STACK_LOCAL`, else local-stack.local.toml
    (gitignored) beside local-stack.toml."""
    return os.environ.get("LOCAL_STACK_LOCAL") or os.path.join(ROOT, "local-stack.local.toml")


def merged(base, over):
    out = dict(base)
    for k, v in over.items():
        out[k] = merged(out[k], v) if isinstance(v, dict) and isinstance(out.get(k), dict) else v
    return out


def settings():
    with open(os.path.join(ROOT, "local-stack.toml"), "rb") as f:
        value = tomllib.load(f)
    try:
        with open(local_file(), "rb") as f:
            value = merged(value, tomllib.load(f))
    except FileNotFoundError:
        pass
    except tomllib.TOMLDecodeError as e:
        die(f"{local_file()}: {e}")
    return value


def get(key, *default):
    value = settings()
    for part in key.split("."):
        if not isinstance(value, dict) or part not in value:
            if default:
                print(default[0])
                return
            die(f"no setting {key} (local-stack.toml, {local_file()})")
        value = value[part]
    print(str(value).lower() if isinstance(value, bool) else value)


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
    names = sorted({p["name"] for p in locked(url)} | declared(url))
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


def yaml_scalar(raw):
    """A one-line YAML scalar's value: 'single' ('' an apostrophe), "double", or plain."""
    v = raw.strip()
    if len(v) >= 2 and v[0] == v[-1] == "'":
        return v[1:-1].replace("''", "'")
    if len(v) >= 2 and v[0] == v[-1] == '"':
        return json.loads(v)
    return v


def seed_share(vault, setlist, code, vault_id):
    """The share note SHARE_CREATE writes for STUDIO_SETLISTS_SHARE: the
    setlist's note and each of its songs' notes, for anyone with the link
    (apps/architect/tests/node.rs builds the same)."""
    if not re.fullmatch(r"[abcdefghijkmnpqrstuvwxyz23456789]{16}", code):
        die(f"share_code `{code}` is not 16 characters of abcdefghijkmnpqrstuvwxyz23456789")
    path = os.path.join(vault, setlist)
    try:
        text = open(path, encoding="utf-8").read()
    except OSError as e:
        die(f"the setlist {setlist} is not in {vault}: {e}")
    fm = frontmatter(text)
    title = next((yaml_scalar(l.split(":", 1)[1]) for l in fm.splitlines() if l.startswith("title:")), None)
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
                    # `Song#Arrangement` names the song's note: the share
                    # holds the note, the arrangement is a heading on it.
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
        "listen": "Reference",
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
    }.get(cmd, lambda *_: die(__doc__))(*args)


if __name__ == "__main__":
    main(sys.argv)
