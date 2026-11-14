import numpy as np, warnings, sys
from scipy import signal as sg
from zoom import pluck,cents,FD
warnings.filterwarnings('ignore')
f=lambda m:440*2**((m-69)/12)
TUN={'6-std':[40,45,50,55,59,64],'drop-d':[38,45,50,55,59,64],'7-std':[35,40,45,50,55,59,64],'bass-4-std':[28,33,38,43],'bass-5-std':[23,28,33,38,43]}
def strum(midi,offs,spread=0.02,rolloff=1.0,Binh=1e-4,dur=1.6,seed=0,noise=1e-3,glide=0):
    x=np.zeros(int(dur*FD)); 
    for i,(m,o) in enumerate(zip(midi,offs)):
        x+=pluck(f(m)*2**(o/1200),dur,Binh=Binh,t0=0.05+spread*i/(len(midi)-1),phase_seed=seed*10+i,rolloff=rolloff,amp=0.12,glide=glide)
    return x+noise*np.random.default_rng(seed).standard_normal(len(x))
def est(x,ft,t_end,T,win='blackmanharris',grid=2.5,band=60):
    # two half-overlapping frames of length 2T/3 inside [t_end-T, t_end]; grid peak on the full window
    i1=int(t_end*FD); n=int(T*FD); seg=x[i1-n:i1]; t=np.arange(n)/FD
    cs=np.arange(-band,band+1e-9,grid); fs=ft*2**(cs/1200)
    w=sg.get_window(win,n,fftbins=False)
    P=np.array([abs((w*seg*np.exp(-2j*np.pi*fq*t)).sum()) for fq in fs]); k=int(P.argmax()); f0=fs[k]
    nL=2*n//3; H=n-nL; wl=sg.get_window(win,nL,fftbins=False)
    for _ in range(2):
        e=np.exp(-2j*np.pi*f0*t)
        A=(wl*seg[:nL]*e[:nL]).sum(); B=(wl*seg[H:]*e[H:]).sum()
        f0=f0+np.angle(B*np.conj(A))/(2*np.pi*H/FD)
    return f0,P[k],(k==0 or k==len(cs)-1)
def trial(tname,times,Tp_fn,rolloff=1.0,seeds=12,win='blackmanharris',rng_off=30,glide=0):
    midi=TUN[tname]; n=len(midi); err={t:np.zeros((seeds,n)) for t in times}
    for s in range(seeds):
        offs=np.random.default_rng(100+s).uniform(-rng_off,rng_off,n)
        x=strum(midi,offs,rolloff=rolloff,seed=s,glide=glide); t_last=0.05+0.02
        for i,m in enumerate(midi):
            ft=f(m); true=ft*2**(offs[i]/1200)*np.sqrt(1+1e-4)
            for t in times:
                T=min(Tp_fn(ft,i),t-0.03)   # window starts >= 30 ms after the first string's onset
                fe,_,_=est(x,ft,0.05+t,T,win=win)
                err[t][s,i]=cents(fe,true)
    print(f"{tname} rolloff 1/k^{rolloff} glide {glide}: median|err| / max|err| per string (cents)")
    for t in times:
        print(f"  t={t:.2f}s: "+"  ".join(f"{np.median(np.abs(err[t][:,i])):5.2f}/{np.abs(err[t][:,i]).max():5.1f}" for i in range(n)))
if __name__=='__main__':
    times=(0.3,0.5,0.75,1.0,1.4)
    design=lambda ft,i: np.clip(40/ft,0.25,1.0)
    for tn in ('6-std','drop-d','7-std','bass-4-std','bass-5-std'):
        trial(tn,times,design)
    trial('6-std',times,design,rolloff=0.5)
    trial('bass-5-std',times,design,rolloff=0.5)
    trial('6-std',times,design,glide=8)
