#!/usr/bin/env python3
"""A vault whose share link opens in Studio's engine, from prepared songs.

    python3 -I scripts/link-vault.py <songs folder> <out vault> [--setlist FILE]
        [--library VAULT] [--title TITLE] [--masters]

`<songs folder>` holds one folder per song, each prepared by `link-prepare`
(features/engine/engine/src/bin/link-prepare.rs): its `.session` with its log
folded in, a rendition of every recording it plays in a `Proxies/` beside
the master (Ogg Vorbis and its page index) and its reference bounce in
`Media/Proxies/`. `--setlist` is the set's order, one song folder per line
(`#` comments), as `studio` reads a `.setlist`; a comment right before a
song line names a set (an act), and the setlist note gets one set each.
Without it, every folder in name order. `--title` names the set (and its
note) instead of the `.setlist`'s file name.

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

`--library VAULT`: the songs already have notes there (a library with
their charts, lyrics and renders). A song folder goes to the note whose
`media:` links `<folder name>.ogg` (a show's renders are named by its song
folders), else to the note with its project's title; the session goes into
that note's folder and the note is written again with the session's
fields and links added, its own files (chart, pages, lyrics, art, renders)
copied beside it. Merged after that vault, this one's notes win. A file
the library note names only in its frontmatter (an arrangement's chart,
its pages) is linked in its body too, so a link serves it; audio and
video it does not link in its body stay off the note's links.

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


def frontmatter_end(text):
    """Where a note's frontmatter closes (the index of its closing `\n---`),
    or None when it has none."""
    if not text.startswith("---\n"):
        return None
    end = text.find("\n---", 3)
    return end if end > 0 else None


# A recording or a video, by its extension.
MEDIA = (".ogg", ".mp3", ".wav", ".flac", ".aif", ".aiff", ".m4a", ".mp4", ".mov", ".mkv", ".webm")

# A wikilink's target: `[[path]]`, `[[path|label]]`, `[[path#part|label]]`.
WIKILINK = re.compile(r"\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|[^\]]*)?\]\]")


def links_of(text):
    """Every vault path a note links, its frontmatter's (single-quoted
    YAML: an apostrophe doubled) and its body's, in order, once each."""
    end = frontmatter_end(text)
    head, body = (text[:end], text[end:]) if end else ("", text)
    out = []
    for part, unquote in ((head, True), (body, False)):
        for m in WIKILINK.finditer(part):
            path = m.group(1).strip()
            path = path.replace("''", "'") if unquote else path
            if path not in out:
                out.append(path)
    return out


def library_songs(library):
    """The song notes of a vault (`--library`), by what names their song
    folder: `slug:<name>` for each `<name>.ogg` the note's `media:`
    links (the show's renders are named by the song folder), and
    `title:<title>` lowercased. Each maps to the note's vault path."""
    found = {}
    for root, dirs, names in os.walk(library):
        dirs[:] = [d for d in dirs if not d.startswith((".", "_"))]
        for name in names:
            if not name.endswith(".md"):
                continue
            path = os.path.join(root, name)
            text = open(path, encoding="utf-8").read()
            end = frontmatter_end(text)
            if not end or not re.search(r"^type:\s*song\s*$", text[:end], re.M):
                continue
            rel = os.path.relpath(path, library)
            title = re.search(r"^title:\s*(.+)$", text[:end], re.M)
            if title:
                t = title.group(1).strip()
                t = t[1:-1].replace("''", "'") if len(t) > 1 and t[0] == t[-1] == "'" else t.strip('"')
                found.setdefault("title:" + t.lower(), rel)
            for target in links_of(text[:end]):
                if target.lower().endswith(".ogg"):
                    found.setdefault("slug:" + os.path.splitext(os.path.basename(target))[0].lower(), rel)
    return found


def merged(text, fields, body):
    """A library note with the session's `fields` added to its
    frontmatter and `body` lines at its end."""
    end = frontmatter_end(text)
    head, rest = text[:end], text[end:]
    return head + "\n" + "\n".join(fields) + rest.rstrip("\n") + "\n\n" + "\n".join(body)


def read_setlist(path):
    """A `.setlist`: its songs in order and its sets. A comment line right
    before a song line names a set (`# Act I: …`); other comments are
    notes. Without such a comment, one set of every song."""
    order, sets, pending = [], [], None
    for raw in open(path, encoding="utf-8"):
        line = raw.strip()
        if not line:
            pending = None
            continue
        if line.startswith("#"):
            pending = line.lstrip("#").strip()
            continue
        if pending:
            sets.append((pending, []))
            pending = None
        order.append(line)
        if sets:
            sets[-1][1].append(line)
    return order, sets


def main(argv):
    flags = {"--setlist": None, "--library": None, "--title": None}
    args, masters, rest = [], False, list(argv[1:])
    while rest:
        a = rest.pop(0)
        if a in flags:
            if not rest:
                die(f"{a} needs a value")
            flags[a] = rest.pop(0)
        elif a == "--masters":
            masters = True
        elif a.startswith("--"):
            die(f"unknown option {a}")
        else:
            args.append(a)
    if len(args) != 2:
        die("usage: link-vault.py <songs folder> <out vault> [--setlist FILE] [--library VAULT] [--title TITLE] [--masters]")
    songs_dir, out = (os.path.abspath(a) for a in args)
    setlist_file, library = flags["--setlist"], flags["--library"]
    library = os.path.abspath(library) if library else None

    if setlist_file:
        order, acts = read_setlist(setlist_file)
        set_title = os.path.splitext(os.path.basename(setlist_file))[0]
    else:
        order = sorted(d for d in os.listdir(songs_dir) if os.path.isdir(os.path.join(songs_dir, d)))
        acts = []
        set_title = os.path.basename(songs_dir)
    set_title = flags["--title"] or set_title
    known = library_songs(library) if library else {}

    rows, notes, total = [], {}, [0, 0, 0]
    for slug in order:
        folder = os.path.join(songs_dir, slug)
        found = song_files(folder) if os.path.isdir(folder) else None
        if found is None:
            print(f"link-vault: {slug}: no .session (prepare it first), left out", file=sys.stderr)
            continue
        project, reference, renditions = found
        title = os.path.splitext(project)[0]
        wal = os.path.join(folder, project + "-wal")
        if os.path.isfile(wal) and os.path.getsize(wal) > 32:
            print(f"link-vault: {slug}: {project} has an unfolded log ({os.path.getsize(wal)} bytes of -wal): "
                  "open it once (link-prepare) so the copy is whole", file=sys.stderr)
        lib_note = known.get("slug:" + slug.lower()) or known.get("title:" + title.lower())
        if library and not lib_note:
            print(f"link-vault: {slug}: no song note of {library} names it: a note of its own", file=sys.stderr)
        if lib_note:
            # The library's own note, its folder the song's: the session
            # goes beside the chart and lyrics it already attaches.
            note = lib_note
            base = os.path.dirname(lib_note)
            lib_text = open(os.path.join(library, lib_note), encoding="utf-8").read()
            title_m = re.search(r"^title:\s*(.+)$", lib_text[: frontmatter_end(lib_text)], re.M)
            if title_m:
                t = title_m.group(1).strip()
                title = t[1:-1].replace("''", "'") if len(t) > 1 and t[0] == t[-1] == "'" else t.strip('"')
        else:
            # A vault path the playhead takes: none of `%`, `?`, `#`.
            safe = re.sub(r"[%?#]", "", title).strip() or slug
            base = f"Songs/{safe}"
            note = f"{base}/{safe}.md"
            lib_text = None
        for rel in [project] + reference + renditions + (masters_of(folder) if masters else []):
            copy(os.path.join(folder, rel), os.path.join(out, base, rel))
        recording = next((r for r in reference if r.lower().endswith("reference.ogg")), None)
        seconds = ogg_seconds(os.path.join(folder, recording)) if recording else None
        stems = [r for r in renditions if r.lower().endswith(".ogg")]
        total[0] += 1
        total[1] += len(stems)
        total[2] += sum(os.path.getsize(os.path.join(folder, r)) for r in reference + renditions)
        fields = [f"session: {link(f'{base}/{project}')}"]
        if recording and not lib_text:
            fields += ["media:", f"- {link(f'{base}/{recording}')}"]
        if reference:
            fields += ["reference:"] + [f"- {link(f'{base}/{r}')}" for r in reference]
        if renditions:
            fields += ["renditions:"] + [f"- {link(f'{base}/{r}')}" for r in renditions]
        if seconds:
            fields += [f"duration: {seconds:.1f}"]
        # A note's attachments are the files its BODY links (Architect's
        # listing reads `LinkIndex::outgoing`, which frontmatter links are
        # not in): each is linked here, by name.
        body = ["## Session", ""]
        body += [f"- [[{base}/{rel}|{os.path.basename(rel)}]]" for rel in [project] + reference + renditions]
        body += [""]
        if lib_text:
            # The library note's own files (chart, its pages, lyrics, art,
            # renders), copied so this vault stands alone; the ones only its
            # frontmatter names are linked in the body too, so a link serves
            # them (an arrangement's chart, say).
            in_body = links_of(lib_text[frontmatter_end(lib_text):])
            unlinked = []
            for target in links_of(lib_text):
                src = os.path.join(library, target)
                if target.endswith(".md") or not os.path.isfile(src):
                    continue
                copy(src, os.path.join(out, target))
                # Audio and video stay where the library put them: a file
                # attached here is on every link of the note, and the
                # library chose not to serve those.
                if target not in in_body and not target.lower().endswith(MEDIA):
                    unlinked.append(target)
            if unlinked:
                body += ["## Files", ""] + [f"- [[{t}|{os.path.basename(t)}]]" for t in unlinked] + [""]
            text = merged(lib_text, fields, body)
        else:
            lines = ["---", "type: song", "title: " + link(title)[3:-3].split("|")[0].join(["'", "'"])]
            lines += fields + ["---", f"# {title}", "", f"{len(stems)} tracks.", ""] + body
            text = "\n".join(lines)
        os.makedirs(os.path.dirname(os.path.join(out, note)), exist_ok=True)
        with open(os.path.join(out, note), "w", encoding="utf-8") as f:
            f.write(text)
        notes[slug] = (title, note[:-3])
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
    item = lambda title, path: f"- '[[{(path + '|' + title).replace(chr(39), chr(39) * 2)}]]'"
    acts = [(name, [s for s in slugs if s in notes]) for name, slugs in acts] or [(set_title, list(notes))]
    sets = []
    for name, slugs in acts:
        if slugs:
            sets += [f"- name: {quoted(name)}", "  songs:"] + ["  " + item(*notes[s]) for s in slugs]
    setlist = [
        "---",
        "sets:",
        *sets,
        "songs:",
        *[item(*v) for v in notes.values()],
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
