import numpy as np, sys
from scipy import signal as sg
from scipy.ndimage import maximum_filter1d
fs=48000
rng=np.random.default_rng(1)
def t60(f): return float(np.interp(np.log(f),np.log([100,4000]),[4.0,1.0]))
def pluck(f0,t0,amp,dur,detune=0.0):
    n=int(dur*fs); t=np.arange(n)/fs; y=np.zeros(n); m=t>=t0
    k=1
    while k*f0<8000:
        f=k*f0*(1+detune); a=amp/k
        y[m]+=a*np.exp(-6.908*(t[m]-t0)/t60(f))*np.sin(2*np.pi*f*(t[m]-t0)+rng.uniform(0,6.28))
        k+=1
    return y
def lr4(fc,kind):
    s=sg.butter(2,fc,kind,fs=fs,output='sos'); return np.vstack([s,s])
def split(x):
    # tree 1k first, then 250 and 4k (magnitudes are what matter here)
    lo=sg.sosfilt(lr4(1000,'low'),x); hi=sg.sosfilt(lr4(1000,'high'),x)
    return [sg.sosfilt(lr4(250,'low'),lo),sg.sosfilt(lr4(250,'high'),lo),sg.sosfilt(lr4(4000,'low'),hi),sg.sosfilt(lr4(4000,'high'),hi)]
def slmax(x,ms):
    w=max(1,int(ms*1e-3*fs)); return maximum_filter1d(np.abs(x),size=w,origin=(w-1)//2,mode='constant')
def gainA(b,win,k,beta=3.0,floor=-40):
    e=20*np.log10(slmax(b,win)+1e-9); P=np.empty_like(e); p=-180.0; d=beta/fs
    for i in range(len(e)):
        p=e[i] if e[i]>p else p-d
        if p<e[i]: p=e[i]
        P[i]=p
    g=np.maximum(k*(e-P),floor)
    a=np.exp(-1/(0.002*fs)); return sg.lfilter([1-a],[1,-a],g),e
def gainB(b,win,k,floor=-40,rearm_db=3.0,rel_ms=60.0,noise=-70.0):
    e=20*np.log10(slmax(b,win)+1e-9)
    a=np.exp(-1/(rel_ms*1e-3*fs)); es=np.empty_like(e); s=-180.0
    r=0.0; R=np.empty_like(e); prev=-180.0; emin=-180.0; r0=0.0
    for i in range(len(e)):
        s=e[i] if e[i]>s else a*s+(1-a)*e[i]
        es[i]=s
        d=s-prev; prev=s
        if s>noise:
            if d<0: r=min(-floor, r-k*d); emin=s; r0=r
            elif d>0: r=r0*max(0.0,1-(s-emin)/rearm_db)
        R[i]=r
    a2=np.exp(-1/(0.002*fs)); return sg.lfilter([1-a2],[1,-a2],-R),es
K=[0.4,0.8,1.2,1.6]
def run(x,scheme):
    bs=split(x); out=np.zeros_like(x); gs=[]
    for i,b in enumerate(bs):
        if scheme=='A': g,e=gainA(b,[25,4,1,0.25][i],K[i])
        else: g,e=gainB(b,25,K[i])
        gs.append(g); out+=b*10**(g/20)
    return out,gs,bs
def envdb(x,ms=30):
    return 20*np.log10(slmax(x,ms)+1e-9)
def slope_t60(x,t1,t2):
    e=envdb(x); i1,i2=int(t1*fs),int(t2*fs); p=np.polyfit(np.arange(i1,i2)/fs,e[i1:i2],1); return -60/p[0]
for sch in 'AB':
    print('=== scheme',sch)
    # 1 single plucks
    for f0 in (82.41,220.0):
        x=pluck(f0,0.05,0.5,2.5); y,gs,bs=run(x,sch)
        ob=split(y)
        r=[]
        for i in range(4):
            if np.max(np.abs(bs[i]))<1e-4: r.append(None); continue
            try: r.append(round(slope_t60(bs[i],0.1,0.1+0.25*[1,1,1,.5][i]+.25)/slope_t60(bs[i]*10**(gs[i]/20),0.1,0.1+0.25*[1,1,1,.5][i]+.25),2))
            except Exception: r.append(None)
        i0=int(0.05*fs); att=[round(float(np.min(g[i0:i0+int(0.005*fs)])),2) for g in gs]
        # ripple: detrended gain pk-pk over 0.3..0.8 s
        rip=[]
        for g in gs:
            seg=g[int(.12*fs):int(.32*fs)]; tt=np.arange(len(seg)); seg=seg-np.polyval(np.polyfit(tt,seg,2),tt); rip.append(round(float(np.ptp(seg)),2))
        print(f'pluck {f0}: T60 in/out per band {r} (target {[1+k for k in K]}); min gain in first 5 ms {att}; gain ripple pk-pk dB {rip}')
    # 3 soft after loud
    x=pluck(110.0,0.05,0.5,2.0)+pluck(130.81,0.65,0.5*10**(-12/20),2.0); y,gs,bs=run(x,sch)
    i0=int(0.65*fs)
    print('soft (-12 dB) note 0.6 s after a loud one: gain just before / mean over its first 20 ms, per band:',[(round(float(g[i0-10]),1),round(float(np.mean(g[i0:i0+int(.02*fs)])),1)) for g in gs])
    # 4 arpeggio, equal level
    x=pluck(110.0,0.05,0.5,2.0)+pluck(164.81,0.65,0.5,2.0); y,gs,bs=run(x,sch)
    print('equal note at 0.65 s: gain before / 20 ms after (old tail re-opened by the difference):',[(round(float(g[i0-10]),1),round(float(g[i0+int(.02*fs)]),1)) for g in gs])
    # 5 beats
    n=int(3*fs); t=np.arange(n)/fs
    for df in (2.0,8.0):
        x=(0.3*np.sin(2*np.pi*440*t)+0.15*np.sin(2*np.pi*(440+df)*t))*np.exp(-6.908*t/3.0); y,gs,bs=run(x,sch)
        def depth(z):
            e=envdb(z,5)[int(1.0*fs):int(2.0*fs)]; tt=np.arange(len(e)); e=e-np.polyval(np.polyfit(tt,e,1),tt); return np.ptp(e)
        print(f'beat {df} Hz (in depth {depth(x):.1f} dB): out depth {depth(y):.1f} dB  (k=0.8 -> x1.8 = {1.8*depth(x):.1f})')
    # 6 dyad intermod: E2+B2 sustained, spurious energy off the two harmonic series
    x=pluck(82.41,0.0,0.4,2.0)+pluck(123.47,0.0,0.4,2.0); y,gs,bs=run(x,sch)
    seg=slice(int(.4*fs),int(.4*fs)+65536)
    def spur(z):
        Z=np.abs(np.fft.rfft(z[seg]*np.hanning(65536)))**2; f=np.fft.rfftfreq(65536,1/fs)
        mask=np.ones_like(Z,bool)
        for f0 in (82.41,123.47):
            for k in range(1,int(8000/f0)+1): mask&=np.abs(f-k*f0)>6
        mask&=(f>60)&(f<6000)
        return 10*np.log10(Z[mask].sum()/Z.sum())
    print(f'dyad E2+B2: off-partial energy in {spur(x):.1f} dB, out {spur(y):.1f} dB re total')
    rip=[]
    for g in gs:
        s2=g[int(.4*fs):int(.9*fs)]; tt=np.arange(len(s2)); s2=s2-np.polyval(np.polyfit(tt,s2,2),tt); rip.append(round(float(np.ptp(s2)),2))
    print('dyad gain ripple pk-pk dB per band',rip)
# sustained (non-decaying) E2: bias of the re-arm rule
n=int(2*fs); t=np.arange(n)/fs
x=sum(0.3/k*np.sin(2*np.pi*82.41*k*t+k) for k in range(1,60))
y,gs,bs=run(x,'B'); print('steady E2, gain at 1.9 s per band',[round(float(g[int(1.9*fs)]),2) for g in gs])
# -80 dBFS noise alone
x=rng.normal(0,10**(-80/20),n); y,gs,bs=run(x,'B'); print('noise -80 dBFS: gain range per band',[(round(float(g.min()),2),round(float(g.max()),2)) for g in gs])
x=rng.normal(0,10**(-60/20),n); y,gs,bs=run(x,'B'); print('noise -60 dBFS: gain range per band',[(round(float(g[fs:].min()),2),round(float(g[fs:].max()),2)) for g in gs])
