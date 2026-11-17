"""RMS (dBFS) per step for each stem: where a part plays, stops, hits or drops out.

usage: ./py/bin/python3 -I stem_energy.py SET.als SAMPLES_DIR LO HI STEP "Track|Track|..."
STEP in beats (1 = per beat, 0.5 = per 8th). Rows are bar.beat from LO.
A `d` after a value = the clip is disabled in the set (played live).
"""
import sys, os, gzip, numpy as np, soundfile as sf, xml.etree.ElementTree as ET
als,sdir,lo,hi,step=sys.argv[1],sys.argv[2],float(sys.argv[3]),float(sys.argv[4]),float(sys.argv[5])
names=sys.argv[6].split('|')
ls=ET.fromstring(gzip.open(als).read()).find('LiveSet')
tempo=[]
for env in ls.find('MainTrack').iter('AutomationEnvelope'):
    if env.find('EnvelopeTarget/PointeeId').get('Value')==ls.find('MainTrack').find('.//Tempo/AutomationTarget').get('Id'):
        tempo=sorted(((float(e.get('Time')),float(e.get('Value'))) for e in env.find('Automation/Events')),key=lambda x:x[0])
def bpm_at(b):
    v=120
    for t,x in tempo:
        if t<=b: v=x
    return v
def clips(nm):
    out=[]
    for tr in ls.find('Tracks'):
        if tr.find('Name/EffectiveName').get('Value')!=nm: continue
        for c in tr.iter('AudioClip'):
            s=float(c.find('CurrentStart').get('Value')); e=float(c.find('CurrentEnd').get('Value'))
            if e<=lo or s>=hi: continue
            rel=c.find('.//SampleRef/FileRef/RelativePath').get('Value')
            wm=sorted((float(w.get('BeatTime')),float(w.get('SecTime'))) for w in c.find('WarpMarkers'))
            out.append(dict(s=s,e=e,f=os.path.join(sdir,os.path.basename(rel)),wm=wm,off=float(c.find('Loop/LoopStart').get('Value'))+float(c.find('Loop/StartRelative').get('Value')),warped=c.find('IsWarped').get('Value')=='true',dis=c.find('Disabled').get('Value')=='true'))
    return out
def b2s(c,b):
    cb=b-c['s']+c['off']
    if not c['warped']:
        # unwarped: seconds = integral of 60/bpm from clip start; offset off is in beats? treat LoopStart as seconds
        secs=0; x=c['s']
        while x<b-1e-9:
            d=min(0.25,b-x); secs+=d*60/bpm_at(x); x+=d
        return c['off']+secs
    wm=c['wm']
    for i in range(len(wm)-1):
        if cb<=wm[i+1][0] or i==len(wm)-2:
            (b0,s0),(b1,s1)=wm[i],wm[i+1]; return s0+(cb-b0)*(s1-s0)/(b1-b0)
cache={}
def load(f):
    if f not in cache:
        y,sr=sf.read(f,always_2d=True); cache[f]=(y.mean(axis=1),sr)
    return cache[f]
CL={n:clips(n) for n in names}
print('step',step,' '.join(n[:10] for n in names))
x=lo
while x<hi-1e-6:
    row=[]
    for n in names:
        v='   . '
        for c in CL[n]:
            if c['s']<=x<c['e']:
                y,sr=load(c['f']); t0,t1=b2s(c,x),b2s(c,x+step)
                seg=y[int(t0*sr):int(t1*sr)]
                if len(seg): 
                    db=20*np.log10(np.sqrt(np.mean(seg**2))+1e-9)
                    v=('%5.0f'%db)+('d' if c['dis'] else ' ')
        row.append(v)
    bar=int((x-lo)//4)+1; beat=(x-lo)%4+1
    print('%4d.%-4g'%(bar,beat),' '.join(row))
    x+=step
