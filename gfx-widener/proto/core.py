import numpy as np, scipy.signal as sg
fs=48000; N=1<<17
f=np.fft.rfftfreq(N,1/fs)
rng=np.random.default_rng(1)
def pink(n,seed=0):
    r=np.random.default_rng(seed); X=np.fft.rfft(r.standard_normal(n)); k=np.arange(len(X)); k[0]=1
    x=np.fft.irfft(X/np.sqrt(k),n); return x/np.std(x)
# ---- 1. exact fold in f32
x=pink(N).astype(np.float32)*np.float32(0.3)
for g in (0.6,2.0):
    s=(pink(N,5)*0.3*g).astype(np.float32)
    L=(x+s).astype(np.float32); R=(x-s).astype(np.float32)
    fold=((L+R)*np.float32(0.5)).astype(np.float32)
    e=fold.astype(np.float64)-x.astype(np.float64)
    print(f"fold g={g}: max err {20*np.log10(np.max(np.abs(e))/np.max(np.abs(x))+1e-300):.1f} dB re peak, rms {20*np.log10(np.std(e)/np.std(x)+1e-300):.1f} dB, exact samples {np.mean(e==0)*100:.1f}%")
# stereo pair input
l=pink(N,2).astype(np.float32)*np.float32(.3); r=pink(N,3).astype(np.float32)*np.float32(.3)
M=((l+r)*np.float32(.5)).astype(np.float32); S=((l-r)*np.float32(.5)).astype(np.float32)*np.float32(1.7)
fold=(((M+S).astype(np.float32)+(M-S).astype(np.float32))*np.float32(.5))
e=fold.astype(np.float64)-M.astype(np.float64); print("stereo src fold err max dB re peak", 20*np.log10(np.max(np.abs(e))/np.max(np.abs(M))))

# ---- generators as impulse responses (length N)
def comb_ir(tau_ms):
    h=np.zeros(N); h[int(round(tau_ms*fs/1000))]=1; return h
def ap2(x,f0,Q):
    w=2*np.pi*f0/fs; al=np.sin(w)/(2*Q)
    b=[1-al,-2*np.cos(w),1+al]; a=[1+al,-2*np.cos(w),1-al]; return sg.lfilter(b,a,x)
def allpass_ir(seed,n=6,flo=300,fhi=10000,qlo=.5,qhi=.9):
    r=np.random.default_rng(seed); h=np.zeros(N); h[0]=1
    fc=np.geomspace(flo,fhi,n)*2**(r.uniform(-.25,.25,n)); q=r.uniform(qlo,qhi,n)
    for a,b in zip(fc,q): h=ap2(h,a,b)
    return h
def velvet_ir(seed,T_ms=20,npulse=30,decay_db=60,logdist=True):
    r=np.random.default_rng(seed); L=T_ms*fs/1000; h=np.zeros(N)
    if logdist:
        edges=L*(np.expm1(np.linspace(0,1,npulse+1)*np.log(10))/9)  # log-ish towards start
    else: edges=np.linspace(0,L,npulse+1)
    pos=np.floor(edges[:-1]+r.uniform(0,1,npulse)*(edges[1:]-edges[:-1])).astype(int)+1
    pos=np.unique(pos); sgn=r.choice([-1,1],len(pos))
    g=10**(-decay_db/20*pos/L)
    h[pos]=sgn*g; return h/np.sqrt(np.sum(h**2))
def bands(kind,flo=200,fhi=16000):
    out=[]
    if kind=='third':
        fc=1000*2**(np.arange(-7,13)/3); fc=fc[(fc>=flo)&(fc<=fhi)]
        return [(c*2**(-1/6),c*2**(1/6),c) for c in fc]
    fc=flo
    while fc<=fhi:
        bw=24.7*(4.37*fc/1000+1); out.append((fc-bw/2,fc+bw/2,fc)); fc+=bw/2
    return out
def band_db(P,kind,flo=200,fhi=16000):
    r=[]
    for lo,hi,c in bands(kind,flo,fhi):
        m=(f>=lo)&(f<hi); r.append(10*np.log10(np.mean(P[m])))
    return np.array(r)
def report(name,h,g=0.6,verbose=True):
    G=np.fft.rfft(h)
    PL=np.abs(1+g*G)**2; PR=np.abs(1-g*G)**2; PS=(PL+PR)/2/(1+g*g)
    res={}
    for kind in ('third','erb'):
        dl=band_db(PL,kind)-10*np.log10(1+g*g); dr=band_db(PR,kind)-10*np.log10(1+g*g)
        ds=band_db(PS,kind)
        res[kind]=(min(dl.min(),dr.min()),max(dl.max(),dr.max()),ds.min(),ds.max())
    # low region detail 200-500
    dl=band_db(PL,'erb',200,500)-10*np.log10(1+g*g)
    # correlation on pink noise 1 s windows
    xs=pink(fs*4,7); s=sg.fftconvolve(xs,h[:fs//4])[:len(xs)]
    Lc=xs+g*s; Rc=xs-g*s; r=np.sum(Lc*Rc)/np.sqrt(np.sum(Lc**2)*np.sum(Rc**2))
    rho=np.sum(xs*s)/np.sqrt(np.sum(xs**2)*np.sum(s**2)); gain=np.std(s)/np.std(xs)
    if verbose:
        print(f"{name:26s} one-side 1/3oct [{res['third'][0]:+.1f},{res['third'][1]:+.1f}] ERB [{res['erb'][0]:+.1f},{res['erb'][1]:+.1f}]  L+R power 1/3oct [{res['third'][2]:+.2f},{res['third'][3]:+.2f}]  r={r:+.3f} (law {(1-g*g)/(1+g*g):+.3f}) rho={rho:+.3f} pinkgain={20*np.log10(gain):+.2f}dB")
    return res,r
if __name__=='__main__':
    print("--- g=0.6, time 20 ms")
    report('comb 20ms',comb_ir(20)); report('comb 12ms',comb_ir(12)); report('comb 5ms',comb_ir(5))
    for s in range(1,5): report(f'allpass6 v{s}',allpass_ir(s))
    for s in range(1,9): report(f'velvet rnd v{s} 20ms/60dB',velvet_ir(s))
    for s in range(1,4): report(f'velvet v{s} 20ms/30dB',velvet_ir(s,decay_db=30))
    for s in range(1,4): report(f'velvet v{s} 20ms/flat',velvet_ir(s,decay_db=0,logdist=False))
    for s in range(1,4): report(f'velvet v{s} 40ms/30dB',velvet_ir(s,T_ms=40,decay_db=30))
    print("--- correlation law, allpass v1 / comb / velvet")
    for g in (0.3,0.6,1,1.5):
        for nm,h in (('allpass',allpass_ir(1)),('comb20',comb_ir(20)),('velvet30dB',velvet_ir(1,decay_db=30))):
            res,r=report(nm,h,g,False); print(f"  g={g} {nm:10s} r={r:+.3f} law={(1-g*g)/(1+g*g):+.3f}")
    # allpass group delay
    h=allpass_ir(1); w,gd=sg.group_delay((h[:8192],[1]),w=np.geomspace(100,12000,60),fs=fs)
    for ff in (150,300,500,1000,3000): print(f"allpass gd at {ff} Hz: {np.interp(ff,w,gd)/fs*1000:.2f} ms")
