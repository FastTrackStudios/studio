"""Match the MusicXML bass line (concert pitch, Bass Trombone part) against the Bass stem per beat.
usage: python bass_match.py <beats.json> <mx.json> <bass.wav>
For each candidate start beat k and transposition t: fraction of score beats whose bass pitch class (+t)
equals the stem's dominant chroma bin in that beat."""
import json, sys
import librosa, numpy as np

beats = np.array(json.load(open(sys.argv[1]))['beats'])
mx = json.load(open(sys.argv[2]))
part = mx['Trombone Basse']
STEPS = {'C': 0, 'D': 2, 'E': 4, 'F': 5, 'G': 7, 'A': 9, 'B': 11}
# score bass pitch class per beat (note sounding at the beat's start; repeat F 49-56 played twice)
order = list(range(0, 56)) + list(range(48, 56)) + list(range(56, len(part)))
sb = []
for mi in order:
    m = part[mi]
    for q in range(4):
        pc = None
        for n in m['notes']:
            if n['midi'] is not None and n['on'] <= q + 1e-6 < n['on'] + n['d']:
                pc = n['midi'] % 12
        sb.append(pc)
sb = np.array([-1 if x is None else x for x in sb])
y, sr = librosa.load(sys.argv[3], sr=22050, mono=True)
C = librosa.feature.chroma_cqt(y=y, sr=sr, hop_length=512, fmin=librosa.note_to_hz('E1'), n_octaves=4)
ct = librosa.frames_to_time(np.arange(C.shape[1]), sr=sr, hop_length=512)
rms = librosa.feature.rms(y=y, hop_length=512)[0]
obs, loud = [], []
for i in range(len(beats) - 1):
    s = (ct >= beats[i]) & (ct < beats[i] + 0.6 * (beats[i + 1] - beats[i]))
    obs.append(int(np.argmax(C[:, s].sum(1))) if s.any() else -1)
    loud.append(rms[s].mean() if s.any() else 0)
obs = np.array(obs); loud = np.array(loud); thr = 0.1 * np.percentile(loud, 90)
res = []
for t in range(12):
    for k in range(len(obs)):
        n = min(len(sb), len(obs) - k)
        if n < 64: break
        a, b, l = sb[:n], obs[k:k + n], loud[k:k + n]
        ok = (a >= 0) & (l > thr)
        if ok.sum() < 64: continue
        res.append(((((a[ok] + t) % 12) == b[ok]).mean(), k, t, int(ok.sum())))
res.sort(reverse=True)
print('observed bass pc per beat (first 80):', ''.join('0123456789ab'[x] if x >= 0 else '.' for x in obs[:80]))
best = {}
for r in res:
    key = (r[1] % 32, r[2])
    if key not in best: best[key] = r
print("best per (start mod 32, transpose):")
for r in sorted(best.values(), reverse=True)[:8]: print(f"  agree {r[0]:.3f} start {r[1]} (mod32 {r[1]%32}) transpose {r[2]:+d}")
for r in res[:4]:
    print(f'agree {r[0]:.3f}  start beat {r[1]:3d} ({beats[r[1]]:.2f}s)  transpose {r[2]:+d}  n={r[3]}')
