import numpy as np
from scipy.signal import butter, sosfilt
from scipy.ndimage import maximum_filter1d
sr=48000
def kick(n, f0=55, dec=0.25, click=True):
    t=np.arange(n)/sr; f=f0+120*np.exp(-t/0.02); ph=2*np.pi*np.cumsum(f)/sr
    y=np.sin(ph)*np.exp(-t/dec)*np.minimum(1,t/0.0005)
    return y
def k808(n): return kick(n, 45, 0.8)
def bass_note(n, f=41.2, a=0.01):
    t=np.arange(n)/sr; return (np.sin(2*np.pi*f*t)+0.4*np.sin(4*np.pi*f*t))*np.minimum(1,t/a)*np.exp(-t/1.5)
def onepole(ms): return np.exp(-1/(ms*0.001*sr))
def det_ks(x, sens=0.5, slow_db=False):
    rise=12-10*sens; ka=onepole(0.2); kr=onepole(5); ks=onepole(150)
    fast=0; slow=1e-7 if not slow_db else -140.; armed=True; refr=0; out=[]
    for i,v in enumerate(np.abs(x)):
        fast = v+(fast-v)*(ka if v>fast else kr)
        fdb=20*np.log10(max(fast,1e-7))
        if slow_db: slow = fdb+(slow-fdb)*ks; sdb=slow
        else: slow = v+(slow-v)*ks; sdb=20*np.log10(max(slow,1e-7))
        refr=max(0,refr-1)
        if armed and refr==0 and fdb-sdb>rise and fdb>-45:
            out.append(i/sr); armed=False; refr=int(0.04*sr)
        elif not armed and fdb-sdb<0.5*rise: armed=True
    return out
def det_max(x, sens=0.5, win_ms=40, slope_ms=3.3):
    R=12-9*sens
    # causal sliding max: use explicit
    w=int(win_ms*0.001*sr); a=np.abs(x); L=maximum_filter1d(a, w, origin=(w-1)//2, mode="constant")
    Ldb=20*np.log10(np.maximum(L,1e-7)); ks=onepole(150); slow=-140.; armed=True; refr=0; out=[]; d=int(slope_ms*0.001*sr)
    for i in range(len(x)):
        slow = Ldb[i]+(slow-Ldb[i])*ks
        refr=max(0,refr-1)
        if armed and refr==0 and Ldb[i]-slow>R and Ldb[i]-Ldb[max(0,i-d)]>R/2 and Ldb[i]>-45:
            out.append(i/sr); armed=False; refr=int(0.04*sr)
        elif not armed and Ldb[i]-slow<0.5*R: armed=True
    return out
n=int(2.0*sr)
tests={}
x=np.zeros(n)
for t0 in [0.1,0.6,1.1,1.6]:
    i=int(t0*sr); 
    seg=kick(int(0.35*sr)); x[i:i+len(seg)]+=0.5*seg
tests["4 kicks (55 Hz, 250 ms)"]=x
x=np.zeros(n); seg=k808(int(1.8*sr)); x[int(0.1*sr):int(0.1*sr)+len(seg)]+=0.5*seg; tests["one long 808"]=x
x=tests["4 kicks (55 Hz, 250 ms)"].copy(); b=np.zeros(n)
for t0 in [0.35,0.85,1.35]:
    i=int(t0*sr); seg=bass_note(int(0.4*sr)); b[i:i+len(seg)]+=seg*np.exp(-np.arange(len(seg))/sr/0.3)
tests["4 kicks + bass bleed -14 dB (3 notes)"]=x+0.1*b
for lp in [None, 150.]:
    print("key lpf", lp)
    for name,x in tests.items():
        y = x if lp is None else sosfilt(butter(2, lp, 'low', fs=sr, output='sos'), x)
        print(f"  {name:40s} killswitch-noHPF {['%.3f'%t for t in det_ks(y)]}")
        print(f"  {'':40s} swell-A-noHPF    {['%.3f'%t for t in det_ks(y, slow_db=True)]}")
        print(f"  {'':40s} sliding-max      {['%.3f'%t for t in det_max(y)]}")
