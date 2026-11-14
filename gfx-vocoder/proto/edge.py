import numpy as np
from bank import erb, bankresp, H
from floor import design
def run(lo,hi,n,sp,fs=48000.,iters=0,wscale=1.0):
    fc,b,s,w=design(lo,hi,n,sp,40.); b=b*wscale
    f=np.geomspace(lo*0.5,min(hi*1.6,fs*0.49),6000); R=bankresp(f,fc,b,fs)
    Rc=bankresp(fc,fc,b,fs)   # at centres
    c=np.ones(n)
    inner=(f>lo*1.3)&(f<hi/1.3)
    G=1/np.sqrt(np.mean(abs(R[:,inner].sum(0))**2))
    for _ in range(iters):
        S=abs((c[:,None]*Rc).sum(0))*G; c=c/S**0.7
    T=20*np.log10(abs((c[:,None]*R).sum(0))*G)
    at=lambda x: T[np.argmin(abs(f-x))]
    full=(f>=lo)&(f<=hi)
    return f"N={n} {sp} it={iters}: inner ripple {T[inner].max()-T[inner].min():.2f}; lo..hi max {T[full].max():+.2f} min {T[full].min():+.2f}; at lo {at(lo):+.2f} at hi {at(hi):+.2f}; hi*1.1 {at(hi*1.1):+.1f} hi*1.2 {at(hi*1.2):+.1f} hi*1.4 {at(hi*1.4):+.1f}; G {20*np.log10(G):+.2f} dB; trims dB lo {20*np.log10(c[0]):+.2f} hi {20*np.log10(c[-1]):+.2f}", fc, b
if __name__=='__main__':
    for n,sp in ((8,'erb'),(16,'erb'),(24,'erb'),(40,'erb'),(11,'log'),(24,'log')):
        for it in (0,1,3,6):
            print(run(100,7000,n,sp,iters=it)[0])
    r,fc,b=run(100,7000,24,'erb'); print('top band fc',fc[-1],'b',b[-1],'spacing',fc[-1]-fc[-2])
    print('width<100 (synth narrower):')
    for ws in (0.5,0.7,1.0):
        print(ws, run(100,7000,24,'erb',iters=3,wscale=ws)[0])
