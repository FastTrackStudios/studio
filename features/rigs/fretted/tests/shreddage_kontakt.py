#!/usr/bin/env python3
"""Render Shreddage Kontakt references in one REAPER process on the Kontakt host.

    python3 shreddage_kontakt.py <mode> <lib> [<lib> …]      e.g.  sus abyss

One project: a track per library and run (r1, r2 — two Kontakt instances,
which should render identically: the engine's only random source at the
shipped settings is a seeded generator, reset when the transport starts),
each with the library's chain (target/fretted-ab/chains/fxchain_<lib>.txt,
Kontakt with the NKI freshly loaded) and shreddage_probe.py's MIDI for
<mode>. Stems land in target/fretted-ab/<lib>/<mode>_kontakt_r<n>.wav beside
the probe. The render REAPER runs on its own copy of the user's settings
(-cfgfile ~/orchestra-ab/cfg/render-shred.ini): it rewrites its ini, which
must never be the user's reaper.ini.

Env: KONTAKT_HOST (default 192.168.1.10, voyager's wired address, checked
against voyager's host key).
"""
import os, re, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", "..", ".."))
sys.path.insert(0, os.path.join(REPO, "features/rigs/orchestra/tests"))
import kontakt_batch as kb  # noqa: E402

HOST = os.environ.get("KONTAKT_HOST", "192.168.1.10")
SSH = ["-o", "HostKeyAlias=voyager", "-o", "BatchMode=yes", "-o", "ConnectTimeout=15"]


def ssh(cmd, timeout=60, check=True):
    return kb.run(["ssh", *SSH, HOST, cmd], timeout, check)


def scp(src, dst, timeout=900, check=True):
    return kb.run(["scp", "-q", *SSH, src, dst], timeout, check)


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    mode, libs = sys.argv[1], sys.argv[2:]
    home = ssh("echo $HOME").stdout.strip()
    rdir = f"{home}/orchestra-ab/batch/shred-{mode}"
    tracks, header, want = [], None, []
    with tempfile.TemporaryDirectory() as tmp:
        for lib in libs:
            d = os.path.join(REPO, "target/fretted-ab", lib)
            if os.path.exists(os.path.join(d, f"{mode}_kontakt_r2.wav")):
                continue
            os.makedirs(d, exist_ok=True)
            mid = os.path.join(d, f"{mode}.mid")
            kb.run(["python3", "-I", os.path.join(HERE, "shreddage_probe.py"), mid, mode], 120)
            chain = os.path.join(REPO, "target/fretted-ab/chains", f"fxchain_{lib}.txt")
            one = os.path.join(tmp, f"{lib}.rpp")
            kb.run(["python3", "-I", os.path.join(REPO, "features/rigs/orchestra/tests/make_ab_rpp.py"),
                    chain, mid, "/dev/null", one], 120)
            text = open(one, encoding="latin-1").read()
            head = text.split("  <TRACK")[0]
            if header is None:
                header = head
            elif kb.tempo_line(head) != kb.tempo_line(header):
                sys.exit(f"{lib}: probe tempo differs from the batch's")
            for r in ("r1", "r2"):
                tracks.append(kb.track_block(text, f"shred_{lib}__{r}"))
                want.append((lib, r, d))
        if header is None or not tracks:
            print(f"{mode}: every render is already there")
            return
        header = re.sub(r'^  RENDER_FILE .*$', f'  RENDER_FILE "{rdir}"', header, flags=re.M)
        header = re.sub(r'^  RENDER_PATTERN .*$', '  RENDER_PATTERN "$track"', header, flags=re.M)
        header = re.sub(r'^  RENDER_STEMS .*$', '  RENDER_STEMS 1', header, flags=re.M)
        rpp = os.path.join(tmp, f"batch_shred_{mode}.rpp")
        open(rpp, "w", encoding="latin-1").write(header + "\n".join(tracks) + "\n>\n")
        ssh(f"rm -rf '{rdir}' && mkdir -p '{rdir}'")
        scp(rpp, f"{HOST}:{rdir}/batch_shred_{mode}.rpp")
    cfg = f"{home}/orchestra-ab/cfg/render-shred.ini"
    ssh(f"cp ~/orchestra-ab/cfg/user-reaper.ini '{cfg}'")
    print(f"{mode}: rendering {len(tracks)} tracks in one REAPER", flush=True)
    t0 = time.time()
    ssh(f"{kb.REAPER} -newinst -nosplash -cfgfile '{cfg}' -renderproject '{rdir}/batch_shred_{mode}.rpp' "
        f">/dev/null 2>&1; for i in $(seq 1200); do "
        f"pgrep -f '^{kb.REAPER} .*/batch_shred_{mode}[.]rpp' >/dev/null || break; sleep 3; done",
        timeout=3900, check=False)
    print(f"{mode}: rendered in {time.time() - t0:.0f}s", flush=True)
    missing = []
    for lib, r, d in want:
        res = scp(f"{HOST}:{rdir}/shred_{lib}__{r}.wav", os.path.join(d, f"{mode}_kontakt_{r}.wav"), check=False)
        if res.returncode != 0:
            missing.append(f"shred_{lib}__{r}")
    if missing:
        sys.exit(f"{mode}: no stem for {' '.join(missing)}")
    print(f"{mode}: {len(want)} stems fetched")


if __name__ == "__main__":
    main()
