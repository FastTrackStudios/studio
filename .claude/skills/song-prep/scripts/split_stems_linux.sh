#!/usr/bin/env bash
# Split a recording into the 11 stems Logic's Stem Splitter gives, on a Linux
# machine with an NVIDIA GPU, in two passes.
#
#   split_stems_linux.sh "<source.wav>" "<dest folder>"
#
# Pass 1, BS-Roformer-SW: vocals, drums, bass, guitar, piano, other.
# Pass 2, MDX23C DrumSep (aufr33/jarredou) on the drums stem: kick, snare,
#         toms, hh, ride, crash.
# Then named as the Logic folders are: "<basename> (<Stem>).wav" for Vocals,
# Bass, Guitar, Piano, Other, Kick, Snare, Hi-Hat, Toms, Cymbals (ride + crash)
# and Drums Other (the drums stem less the six pieces), plus Drums (the whole
# kit) and (Mix), a copy of the source.
#
# Needs: the venv at $STEM_SEP (default /run/media/Development/tools/stem-sep,
# `uv pip install "audio-separator[gpu]"`), ffmpeg (taken from nixpkgs when not
# on PATH), and the GPU driver's libraries (LD_LIBRARY_PATH on NixOS; without
# them it runs on the CPU, about 30 times slower).
set -euo pipefail
src=$1 dest=$2
root=${STEM_SEP:-/run/media/Development/tools/stem-sep}
sep=$root/.venv/bin/audio-separator py=$root/.venv/bin/python
base=$(basename "$src"); base=${base%.*}; base=${base% (Mix)}
work=$(mktemp -d "$root/work.XXXXXX")
export LD_LIBRARY_PATH=/run/opengl-driver/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}
run() { if command -v ffmpeg >/dev/null; then "$@"; else nix shell nixpkgs#ffmpeg -c "$@"; fi; }

"$py" -c 'import torch, sys; ok = torch.cuda.is_available(); print("GPU:", torch.cuda.get_device_name(0) if ok else "none (CPU, slow)")'

run "$sep" "$src" -m BS-Roformer-SW.ckpt --model_file_dir "$root/models" \
  --output_dir "$work/a" --output_format WAV --log_level WARNING
drums=$(ls "$work/a"/*"_(drums)_"*.wav)
run "$sep" "$drums" -m MDX23C-DrumSep-aufr33-jarredou.ckpt --model_file_dir "$root/models" \
  --output_dir "$work/b" --output_format WAV --log_level WARNING

mkdir -p "$dest"
"$py" - "$src" "$work" "$dest" "$base" <<'EOF'
import glob, shutil, sys
import librosa, numpy as np, soundfile as sf
src, work, dest, base = sys.argv[1:5]
def one(d, stem):
    got = glob.glob(f"{work}/{d}/*_({stem})_*.wav")
    assert len(got) == 1, (stem, got)
    return got[0]
ref, sr = sf.read(src, always_2d=True)
n = len(ref)
def fit(a):                       # the source's exact length
    return a[:n] if len(a) >= n else np.vstack([a, np.zeros((n - len(a), a.shape[1]))])
def load(d, stem):
    a, r = sf.read(one(d, stem), always_2d=True)
    if r != sr:                   # the models work at 44.1 kHz: back to the source's rate
        a = librosa.resample(a.T, orig_sr=r, target_sr=sr, res_type="soxr_hq").T
    return fit(a)
out = {
    "Vocals": load("a", "vocals"), "Bass": load("a", "bass"), "Guitar": load("a", "guitar"),
    "Piano": load("a", "piano"), "Other": load("a", "other"), "Drums": load("a", "drums"),
    "Kick": load("b", "kick"), "Snare": load("b", "snare"), "Toms": load("b", "toms"),
    "Hi-Hat": load("b", "hh"),
}
ride, crash = load("b", "ride"), load("b", "crash")
out["Cymbals"] = ride + crash
# The separator turns a loud input down before it works, so its stems come out
# quieter than the source: each pass is scaled back by the one gain that makes
# its stems sum closest to what went in.
def gain(parts, whole):
    total = sum(out[p] for p in parts)
    return float(np.sum(whole * total) / (np.sum(total * total) + 1e-12))
PASS1 = ["Vocals", "Bass", "Guitar", "Piano", "Other", "Drums"]
PIECES = ["Kick", "Snare", "Toms", "Hi-Hat", "Cymbals"]
mix = ref if ref.shape[1] == out["Drums"].shape[1] else ref.mean(1, keepdims=True)
g1 = gain(PASS1, mix)
for p in PASS1: out[p] = out[p] * g1
g2 = gain(PIECES, out["Drums"])
g2 = g2 if 0.5 < g2 < 2.0 else 1.0
for p in PIECES: out[p] = out[p] * g2
print(f"gain back: pass 1 {20 * np.log10(g1):+.2f} dB, drum pieces {20 * np.log10(g2):+.2f} dB")
out["Drums Other"] = out["Drums"] - (out["Kick"] + out["Snare"] + out["Toms"] + out["Hi-Hat"] + out["Cymbals"])
db = lambda a: 20 * np.log10(np.sqrt(np.mean(a ** 2)) + 1e-12)
for name, a in out.items():
    sf.write(f"{dest}/{base} ({name}).wav", np.clip(a, -1, 1), sr, subtype="PCM_24")
    print(f"{name:12s} {len(a) / sr:8.2f} s  rms {db(a):6.1f} dB")
if not glob.glob(f"{dest}/{base} (Mix).wav"):
    shutil.copy(src, f"{dest}/{base} (Mix).wav")
print(f"source       {n / sr:8.2f} s  rms {db(ref):6.1f} dB   -> {dest}")
EOF
rm -r "$work"
