from t6 import dirt,worst_alias,N,win,fs
from lib import *
n=np.arange(2*N)
def note(f0,kmax_hz=8000,slope=1.0,rng=np.random.default_rng(5)):
    K=int(kmax_hz//f0); x=sum((1/k**slope)*np.sin(2*PI*f0*k*n/fs+rng.uniform(0,2*PI)) for k in range(1,K+1))
    return x/np.max(np.abs(x))
def inharm(y,f0s,lo=60,hi=20000):
    Y=np.abs(np.fft.rfft(y[-N:]*win))/(win.sum()/2); f=np.fft.rfftfreq(N,1/fs)
    m=(f>=lo)&(f<=hi)
    # exclude all intermod products a*f1+b*f2 for two tones, or harmonics for one
    if len(f0s)==1:
        k=np.round(f/f0s[0]); m&=np.abs(f-k*f0s[0])>25
    else:
        f1,f2=f0s
        for a in range(-400,401):
            pass
        # products lie on grid k*f2 + j*f1: distance to nearest j*f1 from (f - k*f2)
        ok=np.ones_like(m)
        for k in range(0,8):
            for s in (1,-1):
                r=(f-s*k*f2)/f1; ok&=np.abs(r-np.round(r))*f1>12
        m&=ok
    return 20*np.log10((Y*m).max()+1e-30)
cfgs=(('4x',dict()),('4x+iLP10k',dict(ilp=10000)),('8x',dict(os=8)),('8x+iLP10k',dict(os=8,ilp=10000)))
for desc,kw in cfgs:
    res=[]
    for f0 in (41.27,98.3,196.9,392.4):
        for slope,lab in ((1.0,'bright(1/k to 8k)'),(1.5,'normal')):
            x=db(-6)*note(f0,8000,slope)
            res.append(f'{inharm(dirt(x,48,12,**kw),[f0]):.0f}')
    two=0.4*np.sin(2*PI*41.2*n/fs)+0.2*np.sin(2*PI*3001.7*n/fs)
    print(f'{desc:10s} notes E1/G2/G3/G4 x (bright,normal), -6 dBFS pk, drive 100 %, attack +12:',' '.join(res))
