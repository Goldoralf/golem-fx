import numpy as np
from scipy.signal import lfilter
fs=48000.;N=1<<15
def erb(f): return 24.7*(4.37*f/1000+1)
def erbnum(f): return 21.4*np.log10(1+0.00437*f)
def inv(e): return (10**(e/21.4)-1)/0.00437
def gt(f,bw,order):
    a=np.exp(-2*np.pi*bw/fs+2j*np.pi*f/fs);x=np.zeros(N,complex);x[0]=1
    for _ in range(order): x=lfilter([1-abs(a)],[1,-a],x)
    return x
def run(n,order,w,align):
    e=np.linspace(erbnum(100),erbnum(8000),n);fc=inv(e);sp=e[1]-e[0]
    tot=np.zeros(N);pw=np.zeros(N//2+1)
    gd=[]
    for f in fc:
        y=gt(f,w*erb(f)*sp,order);H=np.fft.fft(y);k=int(round(f/fs*N))
        y=y*np.exp(-1j*np.angle(H[k]))/abs(H[k])
        if align: # delay so that envelope peak aligns to max envelope peak (Hohmann-style)
            pass
        gd.append(np.argmax(abs(y)))
        tot+=np.real(y); pw+=abs(np.fft.rfft(y))**2/4  # real part power approx
    fr=np.fft.rfftfreq(N,1/fs);m=(fr>fc[0]*1.3)&(fr<fc[-1]/1.3)
    d=20*np.log10(abs(np.fft.rfft(tot))[m]);p=10*np.log10(pw[m])
    return d.max()-d.min(),p.max()-p.min(),gd[0]/fs*1e3,gd[len(gd)//2]/fs*1e3,fc[len(fc)//2]
for order in (2,4):
  for n in (16,24,32):
    best=min((run(n,order,w,0)[0],w) for w in np.arange(0.3,2.01,0.1))
    r=run(n,order,best[1],0)
    print(f"order {order} n {n}: best w={best[1]:.1f} amp ripple {r[0]:.2f} dB, power ripple {r[1]:.2f} dB, env peak lowest {r[2]:.1f} ms, mid({r[4]:.0f}Hz) {r[3]:.1f} ms")
