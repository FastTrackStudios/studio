#!/usr/bin/env python3
"""A vault whose share link opens in Studio's engine, from prepared songs.

    python3 -I scripts/link-vault.py <songs folder> <out vault> [--setlist FILE] [--masters]

`<songs folder>` holds one folder per song, each prepared by `link-prepare`
(apps/session/engine/src/bin/link-prepare.rs): its `.session` with its log
folded in, a rendition of every recording it plays in a `Proxies/` beside
the master (Ogg Vorbis and its page index) and its reference bounce in
`Media/Proxies/`. `--setlist` is the set's order, one song folder per line
(`#` comments), as `studio` reads a `.setlist`; without it, every folder
in name order.

Written to `<out vault>` (docs/design/engine-through-a-link.md, "The
vault"):

    Setlists/<Set>.md               type: setlist, its songs in order
    Songs/<Song>/<Song>.md          type: song; attaches the files below
    Songs/<Song>/<Song>.session     the project
    Songs/<Song>/…/Proxies/*.ogg    each rendition, and its `.ogg.idx`
    Songs/<Song>/Media/Proxies/Reference.ogg   the reference bounce

The song note links each of them in its body (and names them in its
frontmatter: `session:`, `reference:`, `renditions:`), which is what makes
them the note's attachments: a link serves them like any other, and the
`[[share_layers]]` rules of Studio's setlists extension say which layer
each is in. The
masters (the WAVs) are left out unless `--masters`: a link plays the
renditions.

Files are copied with reflinks where the filesystem has them. Audio is
never added to git: `<out vault>` belongs outside the repo or under
`target/`.
"""
import os
import re
import shutil
import struct
import subprocess
import sys


def die(msg):
    sys.exit(f"link-vault: {msg}")


def copy(src, dst):
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    if os.path.exists(dst) and os.path.getsize(dst) == os.path.getsize(src):
        return
    done = subprocess.run(["cp", "--reflink=auto", "--", src, dst], check=False)
    if done.returncode != 0:
        shutil.copyfile(src, dst)


def ogg_seconds(path):
    """An Ogg Vorbis file's length: its last page's granule position over
    its sample rate (the identification header's)."""
    try:
        with open(path, "rb") as f:
            head = f.read(4096)
            at = head.find(b"\x01vorbis")
            if at < 0:
                return None
            rate = struct.unpack_from("<I", head, at + 12)[0]
            f.seek(0, os.SEEK_END)
            size = f.tell()
            f.seek(max(0, size - 65536))
            tail = f.read()
        last = tail.rfind(b"OggS")
        if last < 0 or rate == 0:
            return None
        granule = struct.unpack_from("<q", tail, last + 6)[0]
        return granule / rate if granule > 0 else None
    except OSError:
        return None


def link(path):
    """A wikilink to a vault path, as a single-quoted YAML scalar."""
    name = os.path.basename(path)
    return "'[[" + (path + "|" + name).replace("'", "''") + "]]'"


def song_files(folder):
    """A prepared song folder's project, its reference files and its
    renditions, each relative to the folder."""
    project = None
    for name in sorted(os.listdir(folder)):
        if name.lower().endswith(".session"):
            project = name
            break
    if project is None:
        return None
    reference, renditions = [], []
    for root, _dirs, names in os.walk(folder):
        here = os.path.basename(root)
        # A link's coarse waveforms (`Proxies/Peaks/`, link-prepare): the
        # Reference layer's, so the arrangement draws on any link.
        if here == "Peaks" and os.path.basename(os.path.dirname(root)) == "Proxies":
            for name in sorted(names):
                if name.lower().endswith(".sessionpeaks"):
                    reference.append(os.path.relpath(os.path.join(root, name), folder))
            continue
        if here != "Proxies":
            continue
        for name in sorted(names):
            lower = name.lower()
            if not (lower.endswith(".ogg") or lower.endswith(".ogg.idx")):
                continue
            rel = os.path.relpath(os.path.join(root, name), folder)
            (reference if lower.startswith("reference") else renditions).append(rel)
    return project, sorted(reference), sorted(renditions)


def masters_of(folder):
    out = []
    for root, _dirs, names in os.walk(folder):
        if "Proxies" in root.split(os.sep) or "Peaks" in root.split(os.sep):
            continue
        for name in sorted(names):
            if name.lower().endswith((".wav", ".wave", ".aif", ".aiff", ".flac")):
                out.append(os.path.relpath(os.path.join(root, name), folder))
    return out


def main(argv):
    args = [a for a in argv[1:] if not a.startswith("--")]
    flags = [a for a in argv[1:] if a.startswith("--")]
    setlist_file = None
    if "--setlist" in argv:
        setlist_file = argv[argv.index("--setlist") + 1]
        args = [a for a in args if a != setlist_file]
    if len(args) != 2:
        die("usage: link-vault.py <songs folder> <out vault> [--setlist FILE] [--masters]")
    songs_dir, out = (os.path.abspath(a) for a in args)
    masters = "--masters" in flags

    if setlist_file:
        order = [
            line.strip()
            for line in open(setlist_file, encoding="utf-8")
            if line.strip() and not line.lstrip().startswith("#")
        ]
        set_title = os.path.splitext(os.path.basename(setlist_file))[0]
    else:
        order = sorted(d for d in os.listdir(songs_dir) if os.path.isdir(os.path.join(songs_dir, d)))
        set_title = os.path.basename(songs_dir)

    rows, notes, total = [], [], [0, 0, 0]
    for slug in order:
        folder = os.path.join(songs_dir, slug)
        found = song_files(folder) if os.path.isdir(folder) else None
        if found is None:
            print(f"link-vault: {slug}: no .session (prepare it first), left out", file=sys.stderr)
            continue
        project, reference, renditions = found
        title = os.path.splitext(project)[0]
        # A vault path the playhead takes: none of `%`, `?`, `#`.
        safe = re.sub(r"[%?#]", "", title).strip() or slug
        base = f"Songs/{safe}"
        for rel in [project] + reference + renditions + (masters_of(folder) if masters else []):
            copy(os.path.join(folder, rel), os.path.join(out, base, rel))
        recording = next((r for r in reference if r.lower().endswith("reference.ogg")), None)
        seconds = ogg_seconds(os.path.join(folder, recording)) if recording else None
        stems = [r for r in renditions if r.lower().endswith(".ogg")]
        total[0] += 1
        total[1] += len(stems)
        total[2] += sum(os.path.getsize(os.path.join(folder, r)) for r in reference + renditions)
        note = f"{base}/{safe}.md"
        lines = ["---", "type: song", "title: " + link(title)[3:-3].split("|")[0].join(["'", "'"])]
        lines += [f"session: {link(f'{base}/{project}')}"]
        if recording:
            lines += ["media:", f"- {link(f'{base}/{recording}')}"]
        if reference:
            lines += ["reference:"] + [f"- {link(f'{base}/{r}')}" for r in reference]
        if renditions:
            lines += ["renditions:"] + [f"- {link(f'{base}/{r}')}" for r in renditions]
        if seconds:
            lines += [f"duration: {seconds:.1f}"]
        lines += ["---", f"# {title}", "", f"{len(stems)} tracks.", ""]
        # A note's attachments are the files its BODY links (Architect's
        # listing reads `LinkIndex::outgoing`, which frontmatter links are
        # not in): each is linked here, by name.
        lines += ["## Session", ""]
        lines += [f"- [[{base}/{rel}|{os.path.basename(rel)}]]" for rel in [project] + reference + renditions]
        lines += [""]
        os.makedirs(os.path.join(out, base), exist_ok=True)
        with open(os.path.join(out, note), "w", encoding="utf-8") as f:
            f.write("\n".join(lines))
        notes.append((title, note[:-3]))
        rows.append(
            " | ".join(
                [
                    str(len(rows) + 1),
                    title,
                    note,
                    f"{len(stems)} tracks",
                    f"{base}/{recording}" if recording else "",
                    "",
                    f"{seconds:.1f}" if seconds else "",
                ]
            )
        )

    if not notes:
        die(f"no prepared song in {songs_dir}")
    quoted = lambda s: "'" + s.replace("'", "''") + "'"
    listed = [f"- '[[{(path + '|' + title).replace(chr(39), chr(39) * 2)}]]'" for title, path in notes]
    setlist = [
        "---",
        "sets:",
        f"- name: {quoted(set_title)}",
        "  songs:",
        *["  " + l for l in listed],
        "songs:",
        *listed,
        f"title: {quoted(set_title)}",
        "type: setlist",
        "---",
        f"# {set_title}",
        "",
        "```studio-setlist",
        *rows,
        "```",
        "",
    ]
    safe_set = re.sub(r"[%?#/]", "", set_title).strip()
    os.makedirs(os.path.join(out, "Setlists"), exist_ok=True)
    with open(os.path.join(out, "Setlists", f"{safe_set}.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(setlist))
    print(
        f"link-vault: {total[0]} songs, {total[1]} renditions, {total[2] / 1e6:.0f} MB of audio in {out}\n"
        f"link-vault: the setlist is Setlists/{safe_set}.md"
    )


if __name__ == "__main__":
    main(sys.argv)
