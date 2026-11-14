import numpy as np
from scipy import signal
exec(open('xo.py').read().split("f=np.geomspace")[0])
def ap_ir(xs,order,N):
    x=np.zeros(N);x[0]=1
    for fc in xs:
        _,_,ap=split(fc,order)
        for b,a in ap: x=signal.lfilter(b,a,x)
    return x
f=np.geomspace(20,20000,3000)
for order in (4,8):
  for xs in ([40,60,90],[40,2500],[80,600,2500]):
    h=ap_ir(xs,order,1<<16)
    res=[]
    for L in (256,512,1024,2048,4096,8192):
        g=h[:L+1][::-1].copy()
        # half-hann fade over the first 10% (the far tail)
        nf=max(8,L//10); g[:nf]*=np.sin(np.linspace(0,np.pi/2,nf))**2
        # cascade: fir then allpass
        H=signal.freqz(g,1,worN=f,fs=fs)[1]
        for fc in xs:
            _,_,ap=split(fc,order)
            for b,a in ap: H*=signal.freqz(b,a,worN=f,fs=fs)[1]
        E=H-np.exp(-2j*np.pi*f/fs*L)
        res.append('%d:%.0f'%(L,20*np.log10(np.abs(E).max())))
    print(order,xs,' '.join(res))
