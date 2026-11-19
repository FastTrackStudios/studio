import json, numpy as np, librosa, sys
d=json.load(open(sys.argv[1])); bar1=int(sys.argv[2])
b=np.array(d['beats'][bar1:]); st=d['stems']
db=list(b[::4]); 
if len(b)%4==0: db.append(b[-1]+(b[-1]-b[-2]))
else: db.append(b[-1]+(b[-1]-b[-2]))
nb=len(db)-1
N='C Db D Eb E F Gb G Ab A Bb B'.split()
def load(nm): y,sr=librosa.load(st[nm],sr=22050,mono=True); return y,sr
def rms_bars(y,sr): return np.array([20*np.log10(np.sqrt(np.mean(y[int(a*sr):int(c*sr)]**2))+1e-9) if int(a*sr)<len(y) else -99 for a,c in zip(db[:-1],db[1:])])
yb,sr=load('Bass')
C=librosa.feature.chroma_cqt(y=yb,sr=sr,hop_length=512,fmin=librosa.note_to_hz('E1'),n_octaves=3)
def roots(a,c):
    out=[]
    for k in range(2):  # half-bar roots
        t0=a+k*(c-a)/2; f0,f1=librosa.time_to_frames([t0,t0+(c-a)/4],sr=sr,hop_length=512)
        v=C[:,f0:f1+1].mean(1); out.append(N[v.argmax()] if v.max()>0.05 else '-')
    return out
ym,_=librosa.load(d['mix'],sr=22050)
Cm=librosa.feature.chroma_cqt(y=ym,sr=sr,hop_length=512)
F=[]
for a,c in zip(db[:-1],db[1:]):
    f0,f1=librosa.time_to_frames([a,c],sr=sr,hop_length=512); v=Cm[:,f0:f1].mean(1); F.append(v/(np.linalg.norm(v)+1e-9))
F=np.array(F)
R={nm:rms_bars(*load(nm)) for nm in ('Vocals','Guitar','Kick','Snare','Hi-Hat','Cymbals','Other')}
# similarity to bar k-8 and k-16 (repetition)
for k in range(nb):
    r=roots(db[k],db[k+1])
    s8=float(F[k]@F[k-8]) if k>=8 else 0
    print(f"bar{k+1:3} {db[k]:6.1f}s {r[0]:>2}{('/'+r[1]) if r[1]!=r[0] else '':4} vox{R['Vocals'][k]:6.1f} gtr{R['Guitar'][k]:6.1f} kick{R['Kick'][k]:6.1f} snr{R['Snare'][k]:6.1f} hat{R['Hi-Hat'][k]:6.1f} cym{R['Cymbals'][k]:6.1f}")
np.save('feliz_barfeat.npy',F)
