from lib import *
fs=48000
def dirt(x,drive,attack,stages=2,os=4,ilp=None,b=.25,g2=12):
    d=section(x,165,fs,db(-6),1,2); d=section(d,1500,fs,1,db(3+attack),2)
    for _ in range(int(np.log2(os))): d=up2(d)
    d=adaa(d*db(drive),b)
    if stages==2:
        d=section(d,3000,os*fs,1,db(-6),2)
        if ilp: d=section(d,ilp,os*fs,1,0,4)
        d=adaa(d*db(g2),0.0)
    for _ in range(int(np.log2(os))): d=down2(d)
    return section(d,6500,fs,1,0,4)
N=1<<15
win=signal.windows.blackmanharris(N)
def worst_alias(y,f0,lo=60,hi=20000):
    Y=np.abs(np.fft.rfft(y[-N:]*win))/(win.sum()/2); f=np.fft.rfftfreq(N,1/fs)
    m=(f>=lo)&(f<=hi); k=np.round(f/f0); m&=np.abs(f-k*f0)>25
    return 20*np.log10((Y*m).max()+1e-30)
n=np.arange(2*N)
f0s=np.linspace(1000,5200,43)+7.3
for desc,kw in (('4x 2st',dict()),('4x 2st + interstage LP 12k',dict(ilp=12000)),('4x 2st + interstage LP 8k',dict(ilp=8000)),('8x 2st',dict(os=8)),('8x 2st + iLP 12k',dict(os=8,ilp=12000)),('4x 1st',dict(stages=1)),('8x 1st',dict(os=8,stages=1))):
    for amp,at,dr in ((0.5,12,48),(0.25,0,48),(0.25,0,20)):
        r=np.array([worst_alias(dirt(amp*np.sin(2*PI*f0*n/fs),dr,at,**kw),f0) for f0 in f0s])
        print(f'{desc:28s} in {20*np.log10(amp):.0f} dBFS att {at:+d} drive {dr} dB: worst {r.max():.0f}  median {np.median(r):.0f} dBFS')
