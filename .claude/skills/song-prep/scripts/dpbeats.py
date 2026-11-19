"""Globally consistent beats from the stems.

usage: python dpbeats.py <stem folder> <mix.wav> <out.json> [--bpm N] [--tightness T]
1. Combined onset envelope (hop 128 @ 44.1 kHz) = every audible drum stem + half-weight bass,
   each normalised, so beats carried by different kit pieces (hats on 2&4, toms on 1&3) all count.
2. librosa dynamic-programming beat tracker on that envelope (one consistent pulse for the song).
3. Each beat snapped to the strongest drum/bass hit within +-30 ms; no hit -> keep the DP time.
4. Beats after the last hit (a held final chord or a fade) are dropped.
5. Light smoothing (+-1 beat) for the click.
"""
import glob, json, os, sys
from collections import Counter
import librosa
import numpy as np

SR, HOP = 44100, 128
folder, mix, out = sys.argv[1:4]
arg = lambda k, d=None: float(sys.argv[sys.argv.index(k) + 1]) if k in sys.argv else d
bpm, tight = arg('--bpm', 120.0), arg('--tightness', 200.0)
stems = {f.rsplit('(', 1)[1][:-5]: f for f in glob.glob(os.path.join(folder, '*(*).wav'))}
dur = librosa.get_duration(path=mix)
L = int(dur * SR / HOP) + 2
WEIGHT = {'Kick': 1, 'Snare': 1, 'Hi-Hat': 1, 'Toms': 1, 'Cymbals': 0.6, 'Drums Other': 0.6, 'Bass': 0.5}
env = np.zeros(L); used = []
for name, w in WEIGHT.items():
    if name not in stems:
        continue
    a, _ = librosa.load(stems[name], sr=SR, mono=True)
    if 20 * np.log10(np.sqrt(np.mean(a ** 2)) + 1e-12) < -60:
        continue
    o = librosa.onset.onset_strength(y=a, sr=SR, hop_length=HOP, lag=1, max_size=1)
    o = o / (np.percentile(o, 99.5) + 1e-9)
    env += w * np.pad(o, (0, max(0, L - len(o))))[:L]; used.append(name)
print('envelope from:', used)
_, bf = librosa.beat.beat_track(onset_envelope=env, sr=SR, hop_length=HOP, start_bpm=bpm, tightness=tight, trim=False)
dp = librosa.frames_to_time(bf, sr=SR, hop_length=HOP)
pk = librosa.util.peak_pick(env, pre_max=4, post_max=4, pre_avg=30, post_avg=30, delta=0.15, wait=10)
pt, ps = pk * HOP / SR, env[pk]
raw, src = [], []
for t in dp:
    j = np.where(abs(pt - t) < 0.03)[0]
    if len(j):
        raw.append(pt[j[np.argmax(ps[j] - 3 * abs(pt[j] - t) / 0.03)]]); src.append('hit')
    else:
        raw.append(t); src.append('dp')
raw = np.array(raw)
hits = [i for i, s in enumerate(src) if s == 'hit']
last = hits[-1] if hits else len(raw) - 1
raw, src = raw[:last + 1], src[:last + 1]
n = len(raw); idx = np.arange(n); hit = np.array([s == 'hit' for s in src])
fin = np.empty(n)
for i in range(n):
    lo, hi = max(0, i - 1), min(n, i + 2)
    fin[i] = np.polyval(np.polyfit(idx[lo:hi], raw[lo:hi], 1, w=np.where(hit[lo:hi], 1.0, 0.3)), i) if hi - lo > 2 else raw[i]
res = (raw - fin)[hit] * 1000; ibi = np.diff(fin)
print('beats', n, dict(Counter(src)), f'first {fin[0]:.3f}s last {fin[-1]:.3f}s')
print(f'residual to hits rms {np.sqrt((res**2).mean()):.1f} ms, max {abs(res).max():.1f}; tempo {60/ibi.max():.1f}-{60/ibi.min():.1f}, median {60/np.median(ibi):.1f}')
json.dump({'beats': fin.tolist(), 'raw': raw.tolist(), 'src': src, 'sources': used, 'mix': mix,
           'stems': stems}, open(out, 'w'))
