import numpy as np
from scipy.signal import lfilter
from bloom import string, demod, fs
from math import pi
def permax(x,P):
    from scipy.ndimage import maximum_filter1d
    return maximum_filter1d(np.abs(x),P,origin=P//2-1)   # causal sliding max over one period
def run(f0P=582,B=1e-4,g=0.95,bloom_db=-6,T=8.0,t60=4.0,noise_db=-70,freeze_rel_db=-6,K=12):
    f0=fs/f0P; x,f,parts=string(f0,T,B=B,t60=t60,noise_db=noise_db,hum_db=-200,K=K)
    n=len(x); b=permax(x,f0P)[int(.08*fs)]; A=b*10**(bloom_db/20)
    Ld=permax(x,f0P)
    # freeze when the dry falls freeze_rel_db under A
    tf=np.argmax((Ld<A*10**(freeze_rel_db/20))&(np.arange(n)>4800))
    a=np.zeros(f0P+1); a[0]=1; a[-1]=-g
    s=lfilter([1-g],a,x[:tf]); last=s[-f0P:]
    s=np.concatenate([s,np.tile(last,(n-tf)//f0P+1)[:n-tf]])
    Ls=permax(s,f0P)
    w=np.maximum(0,(A-Ld)/np.maximum(Ls,1e-9)); al=1-np.exp(-1/(.3*fs)); w=lfilter([al],[1,-(1-al)],np.minimum(w,8))
    y=x+w*s
    print(f"B={B} g={g} freeze at {tf/fs:.2f}s (dry {freeze_rel_db} dB re A), w at freeze {w[tf]:.2f}, w end {w[-1]:.2f}; out level end {20*np.log10(permax(y,f0P)[-4800]/A):+.2f} dB re A")
    for k in [1,2,3,4,6,8,10]:
        fk,ph,amp=parts[k]
        e=20*np.log10(np.abs(demod(y,ph,bw=12))+1e-12); es=20*np.log10(np.abs(demod(s,ph,bw=12))+1e-12)
        i0=tf-int(.5*fs); 
        img_rel=es[i0]-20*np.log10(amp[i0])   # image partial vs dry partial just before the freeze
        seg=e[tf:tf+int(3*fs)]; 
        from scipy.ndimage import uniform_filter1d
        beat=1200*np.log2(np.sqrt(1+B*k*k)/np.sqrt(1+B))* k*f0/1200*np.log(2)  # Hz approx offset from k*f1
        print(f"  k={k:2d}: image/dry before freeze {img_rel:+5.1f} dB | 3 s after freeze: partial level swing {np.ptp(seg):5.1f} dB | inharm offset {(k*f0*np.sqrt(1+B*k*k)-k*f0*np.sqrt(1+B)):.2f} Hz")
    # hiss between harmonics: band 2.5 f0 .. far: measure noise floor of output vs dry at t>tf+1s in bins away from harmonics
    def floor(z):
        seg=z[tf+int(fs):tf+int(fs)+f0P*64]; S=np.abs(np.fft.rfft(seg*np.hanning(len(seg))))**2; bins=np.arange(len(S)); m=(bins%64>20)&(bins%64<44)&(bins>64*2)&(bins<64*40)
        return 10*np.log10(S[m].mean())
    print(f"  hiss between harmonics, out vs dry: {floor(y)-floor(x):+.1f} dB")
run(B=0.0); run(B=1e-4); run(B=3e-4); run(B=1e-4,freeze_rel_db=-20); run(B=1e-4,g=0.98); run(B=1e-4,f0P=146,t60=5)
