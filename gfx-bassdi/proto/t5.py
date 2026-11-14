from lib import *
fs=48000
def dirt(x,drive,attack,stages=2,os=4,order=2,post=True,inter=True,adaa_on=True):
    d=section(x,165,fs,db(-6),1,order); d=section(d,1500,fs,1,db(3+attack),order)
    for _ in range(int(np.log2(os))): d=up2(d)
    sh=(lambda v,b: adaa(v,b)) if adaa_on else (lambda v,b: np.tanh(v+b)-np.tanh(b))
    d=sh(d*db(drive),.25)
    if stages==2:
        if inter: d=section(d,3000,os*fs,1,db(-6),order)
        d=sh(d*db(12),0.0)
    for _ in range(int(np.log2(os))): d=down2(d)
    if post: d=section(d,6500,fs,1,0,4)
    return d
N=1<<16
win=np.blackman(N)  # sidelobes -58dB.. use 4-term BH
win=signal.windows.blackmanharris(N)
def worst_alias(y,f0,lo=100,hi=20000):
    Y=np.abs(np.fft.rfft(y[-N:]*win))/(win.sum()/2); f=np.fft.rfftfreq(N,1/fs)
    m=(f>=lo)&(f<=hi)
    k=np.round(f/f0); m&=np.abs(f-k*f0)>40
    i=np.argmax(Y*m); return 20*np.log10(Y[i]+1e-30),f[i], 20*np.log10(Y.max())
n=np.arange(3*N)
for desc,kw in (('4x, 2 stages, ADAA, post LP',dict()),('4x, no post LP',dict(post=False)),('8x, 2 stages',dict(os=8)),('8x no post',dict(os=8,post=False)),('4x 1 stage (Vintage-like)',dict(stages=1)),('4x no ADAA',dict(adaa_on=False)),('4x no interstage shelf',dict(inter=False))):
    out=[]
    for f0 in (2003.,3700.,4987.):
      for amp,at,dr in ((0.5,12,48),(0.25,0,48),(0.25,0,27)):
        x=amp*np.sin(2*PI*f0*n/fs)
        a,fa,pk=worst_alias(dirt(x,dr,at,**kw),f0)
        out.append(f'{a:.0f}')
    # two-tone 41+3000
    x=0.4*np.sin(2*PI*41.2*n/fs)+0.2*np.sin(2*PI*3000*n/fs)
    print(f'{desc:30s} worst alias dBFS [f0 2k,3.7k,5k] x [(-6dBFS,att+12,48dB),(-12,0,48),(-12,0,27)]:',' '.join(out))
