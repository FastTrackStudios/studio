import librosa, numpy as np, sys, glob, os
d, mix = sys.argv[1], sys.argv[2]
SR=8000
m,_=librosa.load(mix,sr=SR,mono=True)
for f in sorted(glob.glob(os.path.join(d,'*(*).wav'))):
    s,_=librosa.load(f,sr=SR,mono=True)
    if np.sqrt(np.mean(s**2))<1e-4: print(f"{f.rsplit('(',1)[1][:-5]:12} silent"); continue
    # cross-correlate first 40 s envelope-free: use raw signal segments
    a=m[:SR*40]; b=s[:SR*40]
    n=len(a)+len(b)
    X=np.fft.irfft(np.fft.rfft(a,n)*np.conj(np.fft.rfft(b,n)),n)
    lag=int(np.argmax(X)); lag = lag if lag<n//2 else lag-n
    print(f"{f.rsplit('(',1)[1][:-5]:12} len {len(s)/SR:7.2f}s  offset vs mix {1000*lag/SR:8.1f} ms")
