"""Per-beat stem levels + snare phase check. usage: python snare_check.py <beats.json> [start_beat]"""
import json, sys
import librosa, numpy as np

d = json.load(open(sys.argv[1])); b = np.array(d['beats']); st = d['stems']
k0 = int(sys.argv[2]) if len(sys.argv) > 2 else 0
SR = 22050
def per_beat(name, onset=False):
    a, _ = librosa.load(st[name], sr=SR, mono=True)
    out = []
    for i in range(len(b)):
        e = b[i + 1] if i + 1 < len(b) else b[i] + 0.56
        s0, s1 = int((b[i] - 0.04) * SR), int((b[i] + (0.3 if onset else 1) * (e - b[i])) * SR)
        seg = a[max(0, s0):s1]
        out.append(np.abs(seg).max() if onset else np.sqrt(np.mean(seg ** 2)) if len(seg) else 0)
    return np.array(out)
sn = per_beat('Snare', True); kk = per_beat('Kick', True)
print('first beats (time, kick, snare peak):')
for i in range(min(24, len(b))):
    print(f'{i:3d} {b[i]:7.3f}  k {kk[i]:.3f}  s {sn[i]:.3f}')
ib = np.diff(b)
print('irregular intervals (>4% from local median):')
for i in range(len(ib)):
    med = np.median(ib[max(0, i - 8):i + 8])
    if abs(ib[i] - med) / med > 0.04: print(f'  beat {i}->{i+1} at {b[i]:.2f}s ibi {ib[i]:.3f} vs {med:.3f}')
# snare phase per bar from k0: which beat-in-bar (0..3) has strongest snare; print as string
thr = 0.25 * np.percentile(sn, 95)
line = ''
for j in range((len(b) - k0) // 4):
    s = sn[k0 + 4 * j:k0 + 4 * j + 4]
    if s.max() < thr: line += '.'
    elif s[1] > thr and s[3] > thr and s[1] + s[3] > s[0] + s[2]: line += '+'   # 2&4
    elif s[0] + s[2] > s[1] + s[3]: line += 'X'                                 # 1&3
    else: line += '?'
print('snare phase per bar from beat', k0, '(+ = 2&4, X = 1&3, . = no snare):')
for i in range(0, len(line), 40): print(f'  bar {i+1:3d}: {line[i:i+40]}')
