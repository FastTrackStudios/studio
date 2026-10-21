#!/usr/bin/env python3
"""Make the iPad copy of a gig folder: the same set, its audio as Ogg Vorbis.

A gig folder (see `scripts/signal/gig-prep.py`) carries one WAV per song,
which for a whole set runs past a gigabyte — too much to ride inside an app
upload. The gig player decodes Ogg Vorbis too (fts-sample's
`decode-compressed`), so the copy that ships inside the iPad app keeps the
charts and PDFs as they are and re-encodes the audio (libvorbis, q6,
~190 kb/s), rewriting `gig.json` to point at the new files.

    scripts/signal/gig-ipad.py <gig-dir> <out-dir>

Needs ffmpeg with libvorbis on PATH (`nix shell nixpkgs#ffmpeg-headless`).
Songs already encoded (the .ogg newer than its source) are skipped, so a
re-run after a gig.json edit is quick.
"""

import json
import os
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor


def encode(src: str, dst: str) -> str:
    if os.path.exists(dst) and os.path.getmtime(dst) >= os.path.getmtime(src):
        return f"kept  {os.path.basename(dst)}"
    tmp = dst + ".part.ogg"
    subprocess.run(
        ["ffmpeg", "-nostdin", "-loglevel", "error", "-y", "-i", src,
         "-vn", "-c:a", "libvorbis", "-q:a", "6", tmp],
        check=True,
    )
    os.replace(tmp, dst)
    return f"made  {os.path.basename(dst)}"


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    src_dir, out_dir = sys.argv[1], sys.argv[2]
    gig = json.load(open(os.path.join(src_dir, "gig.json")))
    os.makedirs(os.path.join(out_dir, "audio"), exist_ok=True)
    for sub in ("charts", "pdf"):
        if os.path.isdir(os.path.join(src_dir, sub)):
            shutil.copytree(os.path.join(src_dir, sub), os.path.join(out_dir, sub), dirs_exist_ok=True)
    jobs = []
    for song in gig.get("songs", []):
        audio = song.get("audio")
        if not audio:
            continue
        src = os.path.join(src_dir, audio)
        if not os.path.exists(src):
            continue
        rel = os.path.join("audio", os.path.splitext(os.path.basename(audio))[0] + ".ogg")
        song["audio"] = rel
        # Two songs can share one track (a chart's two revisions).
        if all(dst != os.path.join(out_dir, rel) for _, dst in jobs):
            jobs.append((src, os.path.join(out_dir, rel)))
    with ThreadPoolExecutor(max_workers=int(os.environ.get("JOBS", "3"))) as pool:
        for line in pool.map(lambda j: encode(*j), jobs):
            print(line, flush=True)
    with open(os.path.join(out_dir, "gig.json"), "w") as f:
        json.dump(gig, f, indent=1, ensure_ascii=False)
    print(f"{len(jobs)} tracks -> {out_dir}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
