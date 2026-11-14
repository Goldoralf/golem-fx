import numpy as np, sys
from math import tan, pi, atan2, cos, sin, sqrt, atan
fs=48000.0
def string(f0, T, B=1e-4, t60=6.0, bend=None, vib=None, noise_db=-70, hum_db=-60, K=10, seed=1, level=0.25):
    n=int(T*fs); t=np.arange(n)/fs
    cents=np.zeros(n)
    if bend:  # (t0, dur, cents) up, hold .3 s, back
        t0,du,c=bend; up=np.clip((t-t0)/du,0,1); dn=np.clip((t-t0-du-0.3)/du,0,1)
        cents+=c*(0.5-0.5*np.cos(pi*up))-c*(0.5-0.5*np.cos(pi*dn))
    if vib: t0,dep,rate=vib; cents+=dep*np.sin(2*pi*rate*(t-t0))*(t>t0)
    f=f0*2**(cents/1200); x=np.zeros(n); parts={}
    for k in range(1,K+1):
        fk=k*f*np.sqrt(1+B*k*k); ph=2*pi*np.cumsum(fk)/fs+k*0.7
        amp=level/k*10**(-3*t*(1+0.35*(k-1))/t60)
        parts[k]=(fk,ph,amp); x+=amp*np.cos(ph)
    rng=np.random.default_rng(seed)
    x+=10**(noise_db/20)*rng.standard_normal(n)+10**(hum_db/20)*np.sqrt(2)*np.sin(2*pi*50*t)
    return x,f,parts
class SVF:
    def __init__(s): s.a=s.b=0.0
    def set(s,f,Q): s.g=tan(pi*f/fs); s.k=1/Q; s.d=1/(1+s.g*(s.g+s.k))
    def run(s,x):
        v1=(s.a+s.g*(x-s.b))*s.d; v2=s.b+s.g*v1
        s.a=2*v1-s.a; s.b=2*v2-s.b
        return v1*s.k, v2*s.k      # bp (unity at centre), lp/Q
def bp_resp(f,fc,Q):   # complex response of the unity-gain TPT band-pass
    g=tan(pi*fc/fs); w=tan(pi*f/fs); s=1j*w/g
    return (s/Q)/(s*s+s/Q+1)
def run(x, f_true, parts, k=2, bloom_db=0.0, delay=0.3, rise=1.0, lock_floor_db=-50, yin_lag=0.025, yin_err=3.0, fixed_phase_err=0.0, trim=True, seed=3, body=None):
    n=len(x); rng=np.random.default_rng(seed)
    hop=int(0.0027*fs); lag=int(yin_lag*fs)
    s1,s2=SVF(),SVF(); psi=0.0; integ=0.0; engaged=False
    out=np.zeros(n); gen=np.zeros(n); finst=np.zeros(n); dmeas=np.zeros(n); lockv=np.zeros(n)
    fk_true,ph_true,amp_true=parts[k]
    b=body if body else amp_true[int(0.08*fs)]; A=b*10**(bloom_db/20)
    a=None; fyin=f_true[0]; inc=0; offs=0.0; ecur=0.0
    Q=4.0*k; u0=None
    for i in range(n):
        if i%hop==0:
            j=max(0,i-lag); fyin=f_true[j]*2**(rng.uniform(-yin_err,yin_err)/1200)
            fc=k*fyin; s1.set(fc,Q); s2.set(fc,Q)
            f0=fyin; fL=min(30,max(8,f0/8)); wn=2*pi*fL/2.06; kp=2*0.707*wn/fs; ki=wn*wn/(fs*fs)
        y1,_=s1.run(x[i]); bp,lp=s2.run(y1)
        fn=(k*fyin/fs+ (integ if trim else 0))*fs if engaged else k*fyin
        # quadrature: lp lags bp by 90 deg; amplitude ratio tan(w0/2)/tan(w/2)
        r=tan(pi*fn/fs)/tan(pi*k*fyin/fs)
        H=bp_resp(fn,k*fyin,Q)**2 if i%8==0 else H
        z=complex(bp, lp*r)/H         # analytic estimate of the partial: d*exp(j theta)   (lp = -90deg => cos, sin)
        d=abs(z); theta=atan2(z.imag,z.real)
        t=i/fs
        if not engaged and t>0.06:
            engaged=True; psi=theta/(2*pi)   # snap: the generator is silent
        if engaged:
            e=(theta/(2*pi)-psi+0.5)%1.0-0.5
            lock = d> b*10**(lock_floor_db/20)
            if lock:
                integ+=ki*e; integ=max(-0.06*k*fyin/fs,min(0.06*k*fyin/fs,integ))
                inc=k*fyin/fs+integ+kp*e
            else:
                inc=inc   # hold the frequency
            psi=(psi+inc)%1.0; lockv[i]=lock
        if t>=delay:
            if a is None: a=max(d,A*1e-2); 
            u=(a/A)**2
            if u0 is None: u0=min(u,0.5); beta=np.log(3.8616*(1/u0-1))/(2*rise)
            a*=np.exp(beta*(1-u)/fs)
            g=max(0.0,a-d)
        else: g=0.0
        gen[i]=g*cos(2*pi*psi+fixed_phase_err); out[i]=x[i]+gen[i]; finst[i]=inc*fs; dmeas[i]=d
    return out,gen,finst,dmeas,lockv,A
def demod(y,ph,bw=25.0):
    z=y*np.exp(-1j*ph); a=np.exp(-2*pi*bw/fs)
    from scipy.signal import lfilter
    for _ in range(3): z=lfilter([1-a],[1,-a],z)
    return 2*z
def report(name,f0,k,**kw):
    skw={q:kw.pop(q) for q in list(kw) if q in('B','t60','bend','vib','noise_db','hum_db')}
    T=kw.pop('T',6.0)
    x,f,parts=string(f0,T,**skw)
    out,gen,finst,dmeas,lockv,A=run(x,f,parts,k=k,**kw)
    fk,ph,amp=parts[k]; z=demod(out,ph); env=20*np.log10(np.abs(z)+1e-12)
    t=np.arange(len(x))/fs; d0=int((kw.get('delay',0.3))*fs)
    # step at growth start
    step=np.ptp(env[d0-480:d0+960])
    # ripple after bloom: detrend with 200 ms median
    seg=slice(d0+int(0.2*fs),len(x)-4800); e=env[seg]; 
    from scipy.ndimage import uniform_filter1d
    rip=e-uniform_filter1d(e,int(0.4*fs)); 
    cerr=1200*np.log2(finst[seg]/fk[seg]); lk=lockv[seg]>0
    held=np.where(~lk)[0]
    print(f"{name}: A={20*np.log10(A):.1f} dBFS  step@start {step:.2f} dB | ripple p-p {np.ptp(rip[4800:-4800]):.2f} dB | final level {env[-9600]-20*np.log10(A):+.2f} dB re A | "
          f"freq err RMS {np.sqrt(np.mean(cerr[lk]**2)):.2f} c, max {np.abs(cerr[lk]).max():.1f} c | lock lost at t={ (held[0]/fs+ (d0/fs+.2)) if len(held) else float('nan'):.2f}s, true partial then {20*np.log10(amp[seg][held[0]]) if len(held) else 0:.0f} dBFS")
    return x,out,env,finst,fk,dmeas,amp
if __name__=="__main__":
    report("E4 k=2 plain decay       ",329.6,2)
    report("E4 k=2 B=3e-4            ",329.6,2,B=3e-4)
    report("E4 k=3 B=3e-4            ",329.6,3,B=3e-4)
    report("E2 k=4 B=1e-4            ",82.4,4,B=1e-4)
    report("E2 k=6 B=3e-4            ",82.4,6,B=3e-4)
    report("E2 k=6 B=3e-4 no trim    ",82.4,6,B=3e-4,trim=False)
    report("E4 k=2 bend 200c/150ms@1s",329.6,2,bend=(1.0,0.15,200))
    report("E4 k=2 bend late @3.5s   ",329.6,2,bend=(3.5,0.15,200),t60=3.0)
    report("E4 k=2 vib 30c 6Hz       ",329.6,2,vib=(0.5,30,6))
    report("E4 k=1 vib 30c 6Hz       ",329.6,1,vib=(0.5,30,6),bloom_db=-3)
    report("E2 k=1 vib 30c 6Hz       ",82.4,1,vib=(0.5,30,6),bloom_db=-3)
    report("E2 k=2 bend 200c/150ms   ",82.4,2,bend=(1.0,0.15,200))
    report("E4 k=2 phase err 30deg   ",329.6,2,fixed_phase_err=pi/6)
    report("E4 k=2 phase err 90deg   ",329.6,2,fixed_phase_err=pi/2)
    report("E4 k=2 short t60 1.5s,10s",329.6,2,t60=1.5,T=10.0,lock_floor_db=-200)
    report("E4 k=2 t60 1.5 noise -60 ",329.6,2,t60=1.5,T=10.0,lock_floor_db=-200,noise_db=-60)
