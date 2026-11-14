import numpy as np, sys
from scipy.signal import lfilter
fs=48000.
def erb(f): return 24.7*(4.37*f/1000+1)
def erbnum(f): return 21.4*np.log10(1+0.00437*f)
def inv(e): return (10**(e/21.4)-1)/0.00437
def bank(lo,hi,n,width=1.0,order=4):
    e=np.linspace(erbnum(lo),erbnum(hi),n); fc=inv(e)
    sp=(e[1]-e[0]) if n>1 else 1
    return fc, sp
def resp(fc,bw,order,N):
    a=np.exp(-2*np.pi*bw/fs+2j*np.pi*fc/fs)
    x=np.zeros(N);x[0]=1;y=x.astype(complex)
    for _ in range(order): y=lfilter([1-abs(a)],[1,-a],y)  # unity-ish gain per stage
    return y
N=1<<16
for n in (8,16,20,30,40):
  for order in (2,4):
    fc,sp=bank(100,8000,n)
    tot=np.zeros(N)
    for f in fc:
        bw=1.019*erb(f)*sp   # widen with spacing
        y=resp(f,bw,order,N)
        H=np.fft.fft(y); k=int(round(f/fs*N)); ph=np.angle(H[k]); g=1/abs(H[k])
        tot+=np.real(y*np.exp(-1j*ph))*g
    T=np.abs(np.fft.rfft(tot)); fr=np.fft.rfftfreq(N,1/fs)
    m=(fr>fc[0]*1.2)&(fr<fc[-1]/1.2); d=20*np.log10(T[m])
    # envelope delay at lowest & 1k band
    print(f"n={n} order={order} spacing={sp:.2f}ERB ripple={d.max()-d.min():.2f} dB  mean={d.mean():.1f} dB")
