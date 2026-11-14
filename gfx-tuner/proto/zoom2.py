# Variant: wide pre-LP + finite FIR (window L) + weighted phase regression over span R; drift check
import numpy as np
from scipy import signal as sg
from zoom import pluck,cents,FD
C=1200/np.log(2)
class Zoom2:
    def __init__(s,ft,Lp,Rp,win='hann',Bpre=0.5,fd=FD,Tmin=0.0,Tmax=9,holdoff=None):
        s.ft=ft; s.fd=fd
        s.sos=sg.butter(2,Bpre*ft/(fd/2),output='sos')
        s.D=max(1,int(fd/(8*ft))); s.fb=fd/s.D
        T=np.clip((Lp+Rp)/ft,Tmin,Tmax); s.L=T*Lp/(Lp+Rp); s.R=T-s.L; s.T=T
        s.win=win
        s.holdoff=max(3/ft,0.025) if holdoff is None else holdoff
    def run(s,x,t_on,hop=0.01,grow=True):
        n=np.arange(len(x)); z=x*np.exp(-2j*np.pi*s.ft*n/s.fd)
        i0=int(t_on*s.fd); zf=np.zeros(len(x),complex); zf[i0:]=sg.sosfilt(s.sos,z[i0:])
        zb=zf[::s.D]; 
        nL=int(round(s.L*s.fb)); w=sg.get_window(s.win,nL,fftbins=False); w/=w.sum()
        y=np.convolve(zb,w,mode='full')[:len(zb)]   # causal FIR; y[i] uses zb[i-nL+1..i]
        tb=np.arange(len(zb))/s.fb
        tstart=t_on+s.holdoff+0.5/ (0.5*s.ft)*0+ s.L + 2/s.ft   # first valid FIR output: window wholly after holdoff (+2 periods pre-LP)
        tstart=t_on+s.holdoff+2/s.ft+s.L
        sub=max(1,nL//8)  # frames every L/8
        out=[]
        for tn in np.arange(t_on,len(x)/s.fd,hop):
            lo=max(tn-s.R,tstart)
            if tn-lo < (0.25*s.R if grow else 0.999*s.R)-1e-9: out.append((tn,np.nan,np.nan,np.nan)); continue
            i1=min(int(tn*s.fb),len(y)-1); i0_=int(np.ceil(lo*s.fb)); idx=np.arange(i1,i0_-1,-sub)[::-1]
            if len(idx)<3: out.append((tn,np.nan,np.nan,np.nan)); continue
            zz=y[idx]; tt=tb[idx]; ph=np.unwrap(np.angle(zz)); wt=np.abs(zz)**2
            def fit(m):
                W=wt[m].sum(); tm=(wt[m]*tt[m]).sum()/W; pm=(wt[m]*ph[m]).sum()/W
                Stt=(wt[m]*(tt[m]-tm)**2).sum(); sl=(wt[m]*(tt[m]-tm)*(ph[m]-pm)).sum()/Stt
                res=ph[m]-pm-sl*(tt[m]-tm); return sl,np.sqrt((wt[m]*res**2).sum()/W),Stt/W
            sl,rs,stt=fit(np.ones(len(idx),bool))
            f=s.ft+sl/2/np.pi
            h=len(idx)//2
            if h>=2:
                sa,_,_=fit(np.arange(len(idx))<h+ (len(idx)%2)); sb,_,_=fit(np.arange(len(idx))>=h)
                drift=abs(sb-sa)/2/np.pi
            else: drift=np.nan
            # residual-based sigma: frames overlap 7/8 -> ~ (L/8 hop) effective independent count ~ span/L*2+1
            neff=max(2.0,2*(tn-lo)/s.L+1)
            sig=rs/np.sqrt(stt*neff)/2/np.pi
            out.append((tn,f,C*sig/f,C*drift/f))
        return np.array(out)
