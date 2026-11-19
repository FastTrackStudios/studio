"""Compare two stem sets of the same recording, stem by stem.

usage: python compare_stems.py <folder A> <folder B>
Both hold "<basename> (<Stem>).wav". Per stem: each set's level, how well the
two onset envelopes agree (1.0 = the same hits), and the lag at the best
agreement. Then how closely each set's stems sum back to the mix.
"""
import glob, sys
import librosa, numpy as np

A, B = sys.argv[1:3]
SR, H = 22050, 256
STEMS = ["Kick", "Snare", "Hi-Hat", "Toms", "Cymbals", "Drums Other", "Bass", "Guitar", "Piano", "Vocals", "Other"]
def wav(d, s):
    got = glob.glob(f"{d}/*({s}).wav")
    return librosa.load(got[0], sr=SR, mono=True)[0] if got else None
db = lambda a: 20 * np.log10(np.sqrt(np.mean(a ** 2)) + 1e-12)
print(f"{'stem':12s} {'A dB':>7s} {'B dB':>7s} {'onsets':>7s} {'lag ms':>7s}")
sums = {A: 0, B: 0}
for s in STEMS:
    a, b = wav(A, s), wav(B, s)
    if a is None or b is None:
        print(f"{s:12s} missing"); continue
    for d, x in ((A, a), (B, b)):      # stems of differing lengths (hand exports) are padded
        if isinstance(sums[d], int): sums[d] = x.copy()
        else:
            k = max(len(sums[d]), len(x))
            sums[d] = np.pad(sums[d], (0, k - len(sums[d]))) + np.pad(x, (0, k - len(x)))
    ea = librosa.onset.onset_strength(y=a, sr=SR, hop_length=H); eb = librosa.onset.onset_strength(y=b, sr=SR, hop_length=H)
    m = min(len(ea), len(eb)); ea, eb = ea[:m] - ea[:m].mean(), eb[:m] - eb[:m].mean()
    best = (-2.0, 0)
    for lag in range(-20, 21):
        x = ea[max(0, lag):m + min(0, lag)]; y = eb[max(0, -lag):m - max(0, lag)]
        c = float(np.dot(x, y) / (np.linalg.norm(x) * np.linalg.norm(y) + 1e-12))
        if c > best[0]: best = (c, lag)
    quiet = " (both silent)" if db(a) < -60 and db(b) < -60 else ""
    print(f"{s:12s} {db(a):7.1f} {db(b):7.1f} {best[0]:7.2f} {best[1] * H / SR * 1000:7.1f}{quiet}")
for name, d in (("A", A), ("B", B)):
    mix = wav(d, "Mix")
    if mix is None or isinstance(sums[d], int): continue
    n = min(len(mix), len(sums[d]))
    print(f"{name}: stems sum to the mix within {db(mix[:n] - sums[d][:n]) - db(mix[:n]):.1f} dB of its level")
