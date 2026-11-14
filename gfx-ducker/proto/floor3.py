import numpy as np
exec(open('floor2.py').read().split("def run(")[0])
def run(L, thresh=-60, margin=10, up=40, act_up=0.5, fall=6, G=0.15, D=0.08, step=3.0):
    n=len(L); F=-90.; since_act=10**9; d=0; P=-200.; act_s=np.zeros(n); Fs=np.zeros(n)
    for i in range(n):
        Tt=max(thresh,F+margin); act=L[i]>=Tt
        if act:
            d+=1; P=max(P,L[i]); since_act=0
            if d>D*sr: F+=act_up/sr          # a long excursion: singing
        else:
            if d>0 and d<=D*sr: F+=min(max(P-F,0),step)   # a short excursion just ended: a hit, learn it
            d=0; P=-200.; since_act+=1
            if L[i]>F:
                if since_act>G*sr: F=min(L[i],F+up/sr)
            else: F=max(L[i],F-fall/sr)
        act_s[i]=act; Fs[i]=F
    return act_s,Fs
def held(o, hold=0.25):
    out=o.copy(); c=0
    for i in range(len(o)):
        c = int(hold*sr) if o[i] else max(0,c-1); out[i]=o[i] or c>0
    return out
seg=lambda a,b:(t>=a)&(t<b)
ph=seg(4,8)|seg(9,13)|seg(26,30)
for vl in [-23, -18, -28]:
    voice2=np.where(voice>-150, voice+(vl+23), -200.)
    lin=10**(bleed/20)+10**(voice2/20)
    L2=20*np.log10(maximum_filter1d(lin, w, origin=(w-1)//2, mode='nearest'))
    for kw in [dict(), dict(thresh=-40), dict(step=6.0), dict(D=0.12)]:
        o,F=run(L2,**kw); h=held(o)
        print(f"voice {vl} {str(kw):18s}: F at 3.9 {F[int(3.9*sr)]:.1f}, 7.9 {F[int(7.9*sr)]:.1f}, 12.9 {F[int(12.9*sr)]:.1f}, 25.9 {F[int(25.9*sr)]:.1f}, 29.9 {F[int(29.9*sr)]:.1f} | phrases ducked {100*h[ph].mean():.0f} % (raw {100*o[ph].mean():.0f}); bleed-only ducked: 0.5-4 s {100*h[seg(.5,4)].mean():.0f} %, 14-22 s {100*h[seg(14,22)].mean():.0f} %, 22-26 s {100*h[seg(22,26)].mean():.0f} %, 31-40 s {100*h[seg(31,40)].mean():.0f} %")
