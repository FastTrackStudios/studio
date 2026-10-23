#!/usr/bin/env python3
"""Build a Signal gig folder from a gig manifest — see apps/studio/desktop/src/gig/.

    python3 scripts/signal/gig-prep.py \
        --manifest /Volumes/dev-drive/caa-2026/manifest.json \
        --facts scripts/signal/gigs/caa-2026.json \
        --out ~/Music/FastTrackStudio/Gigs/CAA-2026 \
        [--map /Volumes/dev-drive/caa-2026=/local/copy] [--map ...]

The manifest is the one Voyager's inventory writes (STA-146): per song its
chart PDF, Keyflow chart + renders, best practice audio and Reaper session.
`--facts` adds what the band knows (order, key, tempo, guitar part, dates).
`--map OLD=NEW` rewrites path prefixes, for a manifest copied off the
machine that wrote it.

Writes:
    gig.json        the set, in the facts file's order (then any manifest
                    song it doesn't name)
    audio/          one practice mix per song, WAV (m4a converted with
                    afconvert, 48 kHz / 16-bit)
    charts/         the PDF chart rendered to PNG (PDFKit) and the Keyflow
                    page render
    pdf/            the PDFs

Sections come from the session's REAPER regions, moved onto the audio's own
clock (the region time minus where the audio item sits in the project).
macOS only (afconvert, swift). Re-running it is cheap: files already there
with the same size are kept.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def norm(title: str) -> str:
    return re.sub(r"[^a-z0-9]+", "", title.lower())


def safe(name: str) -> str:
    return re.sub(r"[/:]+", "-", name).strip()


def remap(path, maps):
    if not path:
        return None
    for old, new in maps:
        if path.startswith(old):
            return new + path[len(old):]
    return path


def copy(src: Path, dst: Path) -> bool:
    dst.parent.mkdir(parents=True, exist_ok=True)
    if dst.exists() and dst.stat().st_size == src.stat().st_size:
        return True
    shutil.copy2(src, dst)
    return True


def to_wav(src: Path, dst: Path) -> bool:
    dst.parent.mkdir(parents=True, exist_ok=True)
    if dst.exists() and dst.stat().st_size > 0 and dst.stat().st_mtime >= src.stat().st_mtime:
        return True
    r = subprocess.run(
        ["afconvert", "-f", "WAVE", "-d", "LEI16@48000", str(src), str(dst)],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        print(f"  ! afconvert {src}: {r.stderr.strip()}", file=sys.stderr)
        return False
    return True


def pdf_pages(pdf: Path, prefix: Path) -> list:
    prefix.parent.mkdir(parents=True, exist_ok=True)
    existing = sorted(prefix.parent.glob(prefix.name + "-*.png"))
    if existing and all(p.stat().st_mtime >= pdf.stat().st_mtime for p in existing):
        return existing
    r = subprocess.run(
        ["swift", str(HERE / "pdf-to-png.swift"), str(pdf), str(prefix), "1600"],
        capture_output=True, text=True,
    )
    if r.returncode != 0:
        print(f"  ! pdf render {pdf}: {r.stderr.strip()[:300]}", file=sys.stderr)
        return []
    return [Path(line) for line in r.stdout.split("\n") if line.strip()]


MARKER = re.compile(r'^\s*MARKER\s+(\d+)\s+([0-9.eE+-]+)\s+(?:"([^"]*)"|(\S+))\s*(\d+)?')


def sections_of(rpp: Path):
    """REAPER regions (start, end, name) and the first audio item's position."""
    text = rpp.read_text(errors="replace")
    starts, regions = {}, []
    for line in text.splitlines():
        m = MARKER.match(line)
        if not m:
            continue
        idx, pos = m.group(1), float(m.group(2))
        name = m.group(3) if m.group(3) is not None else (m.group(4) or "")
        flags = int(m.group(5) or 0)
        if not flags & 1:
            continue  # a plain marker
        if idx in starts and name == "":
            s_pos, s_name = starts.pop(idx)
            if pos > s_pos:
                regions.append((s_pos, pos, s_name))
        else:
            starts[idx] = (pos, name)
    item = re.search(r"<ITEM\b(.*?)\n\s*>", text, re.S)
    offset = 0.0
    if item:
        m = re.search(r"^\s*POSITION\s+([0-9.eE+-]+)", item.group(1), re.M)
        if m:
            offset = float(m.group(1))
        m = re.search(r"^\s*SOFFS\s+([0-9.eE+-]+)", item.group(1), re.M)
        if m:
            offset -= float(m.group(1))
    out = []
    for start, end, name in sorted(regions):
        a, b = max(0.0, start - offset), max(0.0, end - offset)
        if b > a:
            out.append({"name": name or "Section", "start": round(a, 3), "end": round(b, 3)})
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--manifest", required=True)
    ap.add_argument("--facts")
    ap.add_argument("--out", required=True)
    ap.add_argument("--map", action="append", default=[], help="OLD=NEW path prefix rewrite")
    args = ap.parse_args()

    maps = [tuple(m.split("=", 1)) for m in args.map]
    manifest = json.loads(Path(args.manifest).read_text())
    facts = json.loads(Path(args.facts).read_text()) if args.facts else {}
    out = Path(os.path.expanduser(args.out))
    out.mkdir(parents=True, exist_ok=True)

    by_title = {norm(s["title"]): s for s in manifest.get("songs", [])}
    order = [f for f in facts.get("songs", [])]
    named = {norm(f["title"]) for f in order}
    order += [{"title": s["title"]} for s in manifest.get("songs", []) if norm(s["title"]) not in named]

    songs, problems = [], []
    for n, fact in enumerate(order, start=1):
        m = by_title.get(norm(fact["title"]), {})
        title = fact.get("display") or fact["title"]
        stem = safe(fact["title"])
        song = {
            "n": n,
            "title": title,
            "artist": fact.get("artist") or m.get("artist") or "",
            "key": fact.get("key", ""),
            "tempo": fact.get("tempo", ""),
            "guitar": fact.get("guitar", ""),
            "notes": fact.get("notes", ""),
            # The song's sound, by reference: no "patch" = the library's
            # All-Around profile; "overrides" = this song's own changes on
            # top (STA-186). Facts may carry a "profile"; otherwise every
            # song starts inheriting All-Around with no overrides.
            "profile": {
                **({"patch": fact["profile"]["patch"]} if fact.get("profile", {}).get("patch") else {}),
                "overrides": list(fact.get("profile", {}).get("overrides", [])),
            },
            "charts": [],
            "sections": [],
        }
        print(f"{n:2}. {title}")

        # Audio.
        bpa = m.get("best_practice_audio") or {}
        src = remap(bpa.get("path"), maps)
        if src and Path(src).is_file():
            src = Path(src)
            # One file per recording (Golden's two charts share one).
            dst = out / "audio" / (safe(src.stem) + ".wav")
            ok = copy(src, dst) if src.suffix.lower() == ".wav" else to_wav(src, dst)
            if ok:
                song["audio"] = str(dst.relative_to(out))
                song["audio_kind"] = bpa.get("kind", "")
        elif src:
            problems.append(f"{title}: audio not found at {src}")

        # Chart: the PDF (what the band reads), then the Keyflow render.
        pdf = remap(m.get("chart_pdf"), maps)
        if pdf and Path(pdf).is_file():
            pdf_dst = out / "pdf" / (stem + ".pdf")
            copy(Path(pdf), pdf_dst)
            song["pdf"] = str(pdf_dst.relative_to(out))
            pages = pdf_pages(pdf_dst, out / "charts" / (stem + " - pdf"))
            for i, page in enumerate(pages, start=1):
                label = "Chart" if len(pages) == 1 else f"Chart p{i}"
                song["charts"].append({"label": label, "path": str(Path(page).relative_to(out))})
        elif pdf:
            problems.append(f"{title}: PDF not found at {pdf}")
        renders = [remap(r, maps) for r in m.get("chart_renders") or []]
        page = next((r for r in renders if r and r.endswith(" - page.png") and Path(r).is_file()), None)
        if page:
            dst = out / "charts" / (stem + " - keyflow.png")
            copy(Path(page), dst)
            song["charts"].append({"label": "Keyflow", "path": str(dst.relative_to(out))})

        # Sections, from the session's regions.
        session = m.get("session") or {}
        rpp = remap(session.get("reaper_rpp") if isinstance(session.get("reaper_rpp"), str) else None, maps)
        if rpp and Path(rpp).is_file() and "audio" in song:
            song["sections"] = sections_of(Path(rpp))
        songs.append(song)
        print(f"     audio={song.get('audio', '-')}  charts={len(song['charts'])}  sections={len(song['sections'])}")

    gig = {
        "name": facts.get("name") or manifest.get("generated_by", "Gig"),
        "dates": facts.get("dates", []),
        "note": facts.get("note", ""),
        "source": facts.get("source", ""),
        "songs": songs,
    }
    (out / "gig.json").write_text(json.dumps(gig, indent=1, ensure_ascii=False) + "\n")
    print(f"\nwrote {out / 'gig.json'}: {len(songs)} songs, "
          f"{sum(1 for s in songs if 'audio' in s)} with audio, "
          f"{sum(1 for s in songs if s['charts'])} with a chart, "
          f"{sum(1 for s in songs if s['sections'])} with sections, "
          f"{sum(1 for s in songs if 'patch' not in s['profile'])} inheriting All-Around "
          f"({sum(1 for s in songs if s['profile']['overrides'])} with overrides)")
    for p in problems:
        print("  ! " + p)
    return 0


if __name__ == "__main__":
    sys.exit(main())
