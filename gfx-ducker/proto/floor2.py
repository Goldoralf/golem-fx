import numpy as np
from scipy.ndimage import maximum_filter1d
sr=4000; T=40; n=T*sr; t=np.arange(n)/sr
def hits(period, peak_db, decay, floor_db, t0, t1):
    ph=(t%period); l=np.maximum(peak_db-ph/decay*8.69, floor_db); l[(t<t0)|(t>=t1)]=-200; return l
bleed=np.maximum(hits(0.25,-38,0.04,-62,0,22), hits(0.25,-30,0.04,-54,22,T))
voice=np.full(n,-200.)
vlev=-23  # only 15 dB over the bleed peaks
for a,b in [(4,8),(9,13),(26,30)]:
    w=a
    while w<b:
        dur=0.22+0.1*((w*7)%1); m=(t>=w)&(t<w+dur)
        ph=(t[m]-w); env=np.where(ph<dur-0.12, 0, -(ph-(dur-0.12))/0.12*30)   # 120 ms tail falling 30 dB
        voice[m]=vlev-4*((w*3)%1)+env; w+=dur+0.05
lin=10**(bleed/20)+10**(voice/20); w=int(0.008*sr)
L=20*np.log10(maximum_filter1d(lin, w, origin=(w-1)//2, mode='nearest'))
def run(gate_ms, thresh=-60, margin=10, up=40, act_up=0.5, fall=6):
    F=-90.; under=0; act_s=np.zeros(n); Fs=np.zeros(n)
    for i in range(n):
        Tt=max(thresh,F+margin); act=L[i]>=Tt
        if act: F+=act_up/sr; under=0
        else:
            under+=1
            if L[i]>F:
                if under>gate_ms*sr/1000: F=min(L[i],F+up/sr)
            else: F=max(L[i],F-fall/sr)
        act_s[i]=act; Fs[i]=F
    return act_s,Fs
ph=(t>=4)&(t<8)|(t>=9)&(t<13)|(t>=26)&(t<30)
def held(o, hold=0.25):
    out=o.copy(); c=0
    for i in range(n):
        c = int(hold*sr) if o[i] else max(0,c-1); out[i]=o[i] or c>0
    return out
for g in [0,50,100,200]:
    o,F=run(g); h=held(o)
    seg=lambda a,b:(t>=a)&(t<b)
    print(f"gate {g:3d} ms: F at 3.9 {F[int(3.9*sr)]:.1f}, 7.9 {F[int(7.9*sr)]:.1f}, 12.9 {F[int(12.9*sr)]:.1f}, 25.9 {F[int(25.9*sr)]:.1f}, 29.9 {F[int(29.9*sr)]:.1f} | ducked (with hold 250) in phrases {100*h[ph].mean():.0f} %, raw {100*o[ph].mean():.0f} %; on bleed 14-22 s {100*h[seg(14,22)].mean():.0f} %, 22-26 s {100*h[seg(22,26)].mean():.0f} %, 31-40 s {100*h[seg(31,40)].mean():.0f} %")
