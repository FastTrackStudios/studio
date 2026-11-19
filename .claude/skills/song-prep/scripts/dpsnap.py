"""DP beats (dpbeats.py output) snapped to the merged drum-hit list.
usage: python dpsnap.py <dpbeats.json> <out.json> [--win 0.06] [--from T]
Each beat moves to the strongest drum hit within +-win (all drum stems merged); unsnapped beats are
re-spaced evenly between their snapped neighbours. Beats before --from are dropped (pickups/noise)."""
import json, sys, librosa, numpy as np
from collections import Counter
SR,HOP=44100,64
d=json.load(open(sys.argv[1])); out=sys.argv[2]
arg=lambda k,dflt=None: float(sys.argv[sys.argv.index(k)+1]) if k in sys.argv else dflt
win=arg('--win',0.06); t_from=arg('--from',0.0)
b=np.array([x for x in d['beats'] if x>=t_from-0.03])
H=[]
for nm in ('Kick','Snare','Hi-Hat','Toms','Cymbals','Drums Other'):
    f=d['stems'].get(nm)
    if not f: continue
    a,_=librosa.load(f,sr=SR,mono=True)
    if 20*np.log10(np.sqrt(np.mean(a**2))+1e-12)<-60: continue
    o=librosa.onset.onset_strength(y=a,sr=SR,hop_length=HOP,lag=1,max_size=1); o=o/(np.percentile(o,99.5)+1e-9)
    pk=librosa.util.peak_pick(o,pre_max=8,post_max=8,pre_avg=40,post_avg=40,delta=0.15,wait=20)
    pri=1 if nm in ('Cymbals','Drums Other') else 0
    H+= [(p*HOP/SR,o[p],nm,pri) for p in pk]
H.sort(); ht=np.array([h[0] for h in H]); hs=np.array([h[1] for h in H]); hp=np.array([h[3] for h in H])
win0=arg('--win0',0.12)
for _pass in range(4):
    snap=np.full(len(b),np.nan); who=['between']*len(b)
    for i,t in enumerate(b):
        j=np.where((abs(ht-t)<win0)&(hp==0))[0]   # kit pieces first, wider window
        if not len(j): j=np.where((abs(ht-t)<win)&(hp==1))[0]   # cymbals: only close by
        if len(j):
            k=j[np.argmax(hs[j]-1.5*abs(ht[j]-t)/(win0 if hp[j[0]]==0 else win))]; snap[i]=ht[k]; who[i]=H[k][2]
    # reject snaps that break monotonic / produce extreme intervals vs neighbours
    for _ in range(2):
        for i in range(1,len(b)-1):
            if np.isnan(snap[i]): continue
            l=snap[i-1] if not np.isnan(snap[i-1]) else b[i-1]; r=snap[i+1] if not np.isnan(snap[i+1]) else b[i+1]
            if not (0.8 < (snap[i]-l)/max(r-snap[i],1e-3) < 1.25): snap[i]=np.nan; who[i]='between'
    idx=np.arange(len(b)); g=~np.isnan(snap)
    fin=np.interp(idx,idx[g],snap[g])
    b=fin.copy()
# last beat: the final hit, if a kit-piece hit is within 0.2 s
j=np.where((abs(ht-fin[-1])<0.2)&(hp==0))[0]
if len(j): fin[-1]=ht[j[np.argmax(hs[j])]]; who[-1]=H[j[np.argmax(hs[j])]][2]+' (final hit)'
ibi=np.diff(fin)
print('beats',len(fin),dict(Counter(who)),f'tempo {60/ibi.max():.1f}-{60/ibi.min():.1f} median {60/np.median(ibi):.1f}')
json.dump({**d,'beats':fin.tolist(),'src':who},open(out,'w'))
