import numpy as np, sys
from scipy import signal as sg
FD=12000.0
def pluck(f0,dur,Binh=0.0,glide=0.0,gtau=0.15,amp=0.25,tau1=2.0,fd=FD,fmaxp=5000,t0=0.05,rolloff=1.0,phase_seed=0):
    n=int(dur*fd); t=np.arange(n)/fd-t0; on=t>=0
    rng=np.random.default_rng(phase_seed)
    x=np.zeros(n); g=np.where(on,glide*np.exp(-np.maximum(t,0)/gtau),0)
    ratio=2**(g/1200)
    k=1
    while k*f0*np.sqrt(1+Binh*k*k)<fmaxp and k<60:
        fk=k*f0*np.sqrt(1+Binh*k*k)*ratio
        ph=2*np.pi*np.cumsum(fk)/fd+rng.uniform(0,2*np.pi)
        env=np.exp(-np.maximum(t,0)/(tau1/(1+0.25*(k-1))))*(1-np.exp(-np.maximum(t,0)/0.002))
        x+=on*amp/k**rolloff*env*np.sin(ph)
        k+=1
    return x
def cents(a,b): return 1200*np.log2(a/b)
class Zoom:
    def __init__(s,ft,B,T,fd=FD,order=4,fbmin=50.0,holdoff=None,excl=None):
        s.ft=ft;s.B=B;s.T=T;s.fd=fd
        s.sos=sg.butter(order,B/(fd/2),output='sos')
        s.D=max(1,int(fd/max(4*B,fbmin))); s.fb=fd/s.D
        s.holdoff=max(3/ft,0.025) if holdoff is None else holdoff
        s.excl=(1.66/B) if excl is None else excl
    def run(s,x,t_on,hop=0.01,weights=True):
        n=np.arange(len(x)); z=x*np.exp(-2j*np.pi*s.ft*n/s.fd)
        # restart at onset: zero state from onset+holdoff? filter runs continuously here; reset state at onset
        i0=int(t_on*s.fd)
        zf=np.zeros(len(x),complex)
        zf[i0:]=sg.sosfilt(s.sos,z[i0:])
        zb=zf[::s.D]; tb=np.arange(len(zb))/s.fb
        tstart=t_on+s.holdoff+s.excl
        out=[]
        th=np.arange(t_on,len(x)/s.fd,hop)
        for tn in th:
            lo=max(tn-s.T,tstart)
            m=(tb>lo)&(tb<=tn)
            if m.sum()<6 or (tn-lo)<0.5*s.T:
                out.append((tn,np.nan,np.nan,0)); continue
            zz=zb[m]; tt=tb[m]
            ph=np.unwrap(np.angle(zz)); w=np.abs(zz)**2 if weights else np.ones(len(zz))
            W=w.sum(); tm=(w*tt).sum()/W; pm=(w*ph).sum()/W
            Stt=(w*(tt-tm)**2).sum(); slope=(w*(tt-tm)*(ph-pm)).sum()/Stt
            res=ph-pm-slope*(tt-tm)
            # weighted residual variance, normalised so w~1/var
            s2=(w*res**2).sum()/W*len(zz)/(len(zz)-2)
            sig_slope=np.sqrt(s2/(Stt/W*len(zz)))
            df=slope/2/np.pi; f=s.ft+df
            out.append((tn,f,1200/np.log(2)*sig_slope/2/np.pi/f,tn-lo))
        return np.array(out)
