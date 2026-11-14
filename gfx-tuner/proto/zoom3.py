# Proposed spec: demod at coarse fc, wide pre-LP (BW4 @0.5fc), fb~4fc ring; growing window T (Tmin..Tmax),
# Hann frames L=T/2 every L/8 over R=T/2, |X|^2-weighted phase regression; error bar; span-stability rule.
import numpy as np, warnings
from scipy import signal as sg
from zoom import pluck,cents,FD
from t2 import sigs
warnings.filterwarnings('ignore')
C=1200/np.log(2)
def run(x,fc,t_on,pmin,pmax,Tlo,Thi,fd=FD,hop=0.01):
    n=np.arange(len(x)); z=x*np.exp(-2j*np.pi*fc*n/fd)
    sos=sg.butter(4,0.5*fc/(fd/2),output='sos')
    i0=int(t_on*fd); zf=np.zeros(len(x),complex); zf[i0:]=sg.sosfilt(sos,z[i0:])
    D=max(1,int(fd/(4*fc))); fb=fd/D; zb=zf[::D]; tb=np.arange(len(zb))/fb
    Tmax=np.clip(pmax/fc,Tlo,Thi); Tmin=min(pmin/fc,Tmax)
    tstart=t_on+max(3/fc,0.025)+2/fc
    out=[]; dprev=0.0
    for tn in np.arange(t_on,len(x)/fd-hop,hop):
        T=min(tn-tstart,Tmax)
        if T<Tmin: out.append((tn,np.nan,np.nan,T)); continue
        L=T/2; nL=max(4,int(round(L*fb))); w=np.hanning(nL+2)[1:-1]
        i1=int(tn*fb); sub=max(1,nL//8); ends=np.arange(i1,i1-int(round((T-L)*fb))-1,-sub)[::-1]
        ends=ends[ends-nL+1>=int(np.ceil(tstart*fb))]
        if len(ends)<3: out.append((tn,np.nan,np.nan,T)); continue
        X=np.array([ (w*zb[e-nL+1:e+1]*np.exp(-2j*np.pi*dprev*tb[e-nL+1:e+1])).sum() for e in ends]); tt=tb[ends]
        ph=np.unwrap(np.angle(X)); wt=np.abs(X)**2; W=wt.sum(); tm=(wt*tt).sum()/W; pm=(wt*ph).sum()/W
        Stt=(wt*(tt-tm)**2).sum(); sl=(wt*(tt-tm)*(ph-pm)).sum()/Stt
        res=ph-pm-sl*(tt-tm); rs=np.sqrt((wt*res**2).sum()/W)
        sig=rs/np.sqrt(Stt/W*3.0)/2/np.pi      # ~3 independent frames in the span
        d=float(np.clip(dprev+sl/2/np.pi,-0.02*fc,0.02*fc)); dprev=d
        out.append((tn,fc+d,C*sig/(fc+d),T))
    return np.array(out)
def settled(o,true,tol,sigmax,span_s,t_on=0.05,hop=0.01,stol=None):
    stol=tol if stol is None else stol
    t=o[:,0]-t_on; c=cents(o[:,1],true); ok=~np.isnan(c); k=int(round(span_s/hop))
    st=np.zeros(len(t),bool)
    for i in range(len(t)):
        if i-k<0 or not ok[i-k:i+1].all(): continue
        seg=c[i-k:i+1]; st[i]=(seg.max()-seg.min()<=stol) and (o[i,2]<sigmax)
    first=t[ok][0] if ok.any() else np.nan
    ss=np.where(st)[0]
    bad=np.where(ok&(np.abs(c)>tol)&(t<2.8))[0]
    terr=first if len(bad)==0 else t[min(bad[-1]+1,len(t)-1)]
    return first,terr,(t[ss[0]] if len(ss) else np.nan),(np.nanmax(np.abs(c[st])) if len(ss) else np.nan),st.mean(), c,t
if __name__=='__main__':
    notes={'A0':27.5,'B0':30.868,'E1':41.203,'B1':61.735,'E2':82.407,'A2':110.0,'D3':146.83,'E4':329.63,'E6':1318.5}
    modes={'Fast':(8,24,0.08,0.6,0.3,0.15,0.15),'Strobe':(8,48,0.3,1.2,0.1,0.04,0.30)}
    for mode,(pmin,pmax,Tlo,Thi,tol,sigmax,span) in modes.items():
        print(f"== {mode}: window {pmin}..{pmax} periods clamp [{Tlo},{Thi}] s; settled = span({span}s)<={tol}c & sigma<{sigmax}")
        cases=[]
        for nm,f in notes.items():
            cases+=[(f"{nm} +0c",f,dict(off=0)),(f"{nm} -100c",f,dict(off=-100)),(f"{nm} glide8 B2e-4",f,dict(off=3,Binh=2e-4,glide=8))]
        cases+=[("D3 + E2 ringing -10dB (x2 200c)",146.83,dict(symp=(82.407,-10))),("D3 + E2 ringing 0dB",146.83,dict(symp=(82.407,0))),
                ("A2 + 200c-below -10dB",110.0,dict(symp=(98.0,-10))),("A2 + 200c-below 0dB",110.0,dict(symp=(98.0,0))),("A2 + E2 +10dB",110.0,dict(symp=(82.407,10))),
                ("G2 + B0 ringing (x3 98c) 0dB",98.0,dict(symp=(30.868,0))),("E2 noise -40dB",82.407,dict(noise_db=-40)),("B0 noise -50dB",30.868,dict(noise_db=-50))]
        for nm,f,kw in cases:
            x,true=sigs(f,**{'off':0,**kw})
            target=f; fc=true*2**(4/1200)   # coarse is 4 c off
            o=run(x,fc,0.05,pmin,pmax,Tlo,Thi)
            first,terr,ts,worst,frac,c,t=settled(o,true,tol,sigmax,span)
            late=(t>1.8)&(t<2.8)
            print(f"  {nm:32s} first {first:.2f}s |err|<={tol} from {terr:.2f}s  settled@{ts:.2f}s worst|err| settled {worst:.3f}  late: max|err| {np.nanmax(np.abs(c[late])):.3f} sigma {np.nanmean(o[late,2]):.3f}")
