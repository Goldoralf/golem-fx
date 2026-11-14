import numpy as np, cmath
from math import pi, cos, exp, log
from bloom import string, demod, fs
from scipy.ndimage import uniform_filter1d
from scipy.signal import lfilter
class Het:
    """heterodyne partial tracker: x*exp(-j psi) -> 2 one-poles -> d, e -> PI -> NCO"""
    def __init__(s, lp_ratio=1/3, bw=lambda f0:min(40,max(10,f0/6)), poles=2): s.poles=poles; s.zz=[0j]*poles; s.z1=s.z2=0j; s.psi=0.0; s.integ=0.0; s.lp_ratio=lp_ratio; s.bw=bw; s.inc=0.0; s.on=False; s.e=0.0; s.d=0.0; s.ev=0.0
    def tune(s,f0):
        s.al=1-exp(-2*pi*f0*s.lp_ratio/fs); wn=2*pi*s.bw(f0)/2.06; s.kp=2*0.707*wn/fs; s.ki=wn*wn/fs/fs
    def step(s,x,ff,hold=False):
        m=x*cmath.exp(-2j*pi*s.psi)
        for q in range(s.poles):
            s.zz[q]+=s.al*(m-s.zz[q]); m=s.zz[q]
        z=2*m
        s.d=abs(z); s.e=cmath.phase(z)/(2*pi) if s.d>0 else 0.0
        if not hold: s.integ+=s.ki*s.e
        s.inc=ff+s.integ+(0 if hold else s.kp*s.e); s.psi=(s.psi+s.inc)%1.0
        return s.d
def run(x,f_true,parts,k,bloom_db=0.0,delay=0.3,rise=1.0,yin_lag=0.025,yin_err=3.0,hold_db=-200,seed=3,slave=True,plp=0.4,pbw=lambda f0:4.0,fbw=None,**tk):
    n=len(x); rng=np.random.default_rng(seed); hop=int(.0027*fs); lag=int(yin_lag*fs)
    F=Het(**tk); P=Het(lp_ratio=plp,bw=pbw,poles=4); fsm=None
    amp=parts[k][2]; b=amp[int(.08*fs)]; A=b*10**(bloom_db/20); a=None;u0=None
    out=np.zeros(n); gen=np.zeros(n); finst=np.zeros(n); dm=np.zeros(n); ev=np.zeros(n)
    for i in range(n):
        if i%hop==0:
            fy=f_true[max(0,i-lag)]*2**(rng.uniform(-yin_err,yin_err)/1200); fsm=fy if fsm is None else fsm+(fy-fsm)*(1-exp(-.0027/.03))
            F.tune(fsm); P.tune(fsm)
        F.step(x[i],fsm/fs)
        if k==1: T=F
        else:
            ff=k*F.inc
            P.step(x[i],ff, hold=P.d<b*10**(hold_db/20) and i>4800); T=P
        t=i/fs; d=T.d
        T.ev+=(T.e*T.e-T.ev)*(1/(0.05*fs)); ev[i]=T.ev
        if t>=delay:
            if a is None: a=max(d,A*1e-2)
            u=(a/A)**2
            if u0 is None: u0=min(u,.5); beta=log(3.8616*(1/u0-1))/(2*rise)
            a*=exp(beta*(1-u)/fs); g=max(0.0,a-d)
        else: g=0.0
        # the NCO phase psi lags the input by the phase detector's reading e: output at psi (+e would add jitter)
        gen[i]=g*cos(2*pi*T.psi); out[i]=x[i]+gen[i]; finst[i]=T.inc*fs; dm[i]=d
    return out,gen,finst,dm,ev,A
def rep(name,f0,k,T=5.0,**kw):
    skw={q:kw.pop(q) for q in list(kw) if q in('B','t60','bend','vib','noise_db','hum_db')}
    x,f,parts=string(f0,T,**skw); out,gen,finst,dm,ev,A=run(x,f,parts,k,**kw)
    fk,ph,amp=parts[k]; d0=int(kw.get('delay',.3)*fs); seg=slice(d0+int(.2*fs),len(x)-4800)
    env=20*np.log10(np.abs(demod(out,ph))+1e-12); e=env[seg]; rip=e-uniform_filter1d(e,int(.4*fs))
    pe=np.unwrap(np.angle(demod(gen,ph)[seg]))[4800:-4800]
    al=1-np.exp(-1/(.005*fs)); cs=lfilter([al],[1,-(1-al)],1200*np.log2(finst[seg]/fk[seg]))[4800:]
    step=np.ptp(env[d0-480:d0+960])
    print(f"{name}: step {step:4.2f} dB | ripple p-p {np.ptp(rip[4800:-4800]):5.2f} dB | phase err p-p {np.ptp(pe):5.2f} rad | freq err RMS {np.sqrt(np.mean(cs**2)):5.2f} c max {np.abs(cs).max():5.1f} c | final {env[-9600]-20*np.log10(A):+.2f} dB")
    return dict(x=x,out=out,env=env,finst=finst,fk=fk,amp=amp,dm=dm,ev=ev,gen=gen,ph=ph)

if __name__=="__main__":
    for nm,kw in [("F: het 2-pole f0/3, bw f0/6 (10-40); partial k: k*psi + slow het trim (4-pole f0/2.5, 4 Hz)",dict()),("same, trim 8 Hz",dict(pbw=lambda f0:8.0)),("F bw f0/5 (14-50)",dict(bw=lambda f0:min(50,max(14,f0/5))))]:
        print("==",nm)
        rep("E4 k=2 static     ",329.6,2,**kw)
        rep("E2 k=6 B3e-4 stat ",82.4,6,B=3e-4,**kw)
        rep("E2 k=4 B1e-4 stat ",82.4,4,B=1e-4,**kw)
        rep("E4 k=1 vib30/6    ",329.6,1,vib=(0.5,30,6),bloom_db=-3,**kw)
        rep("E4 k=2 vib30/6    ",329.6,2,vib=(0.5,30,6),**kw)
        rep("E4 k=3 vib50/6    ",329.6,3,vib=(0.5,50,6),**kw)
        rep("E2 k=1 vib30/6    ",82.4,1,vib=(0.5,30,6),bloom_db=-3,**kw)
        rep("E2 k=2 vib30/6    ",82.4,2,vib=(0.5,30,6),**kw)
        rep("E2 k=4 vib30/6    ",82.4,4,vib=(0.5,30,6),B=1e-4,**kw)
        rep("A2 k=3 vib50/6    ",110.0,3,vib=(0.5,50,6),**kw)
        rep("E4 k=2 bend200/150",329.6,2,bend=(1.0,0.15,200),**kw)
        rep("E2 k=2 bend200/150",82.4,2,bend=(1.0,0.15,200),**kw)
        rep("E4 k=4 bend200/150",329.6,4,bend=(1.0,0.15,200),**kw)
