import numpy as np, sc
fs=48000
rng=np.random.default_rng(3)
def pink(n,lev):
    X=np.fft.rfft(rng.standard_normal(n)); f=np.fft.rfftfreq(n,1/fs); X[1:]/=np.sqrt(f[1:]); X[0]=0
    y=np.fft.irfft(X,n); return y*10**(lev/20)/np.std(y)
print("== musical noise: pink noise -18 dBFS RMS, thresh -40, down 4, up 4, attack 10, release 50, N 2048 (Studio)")
x=pink(4*fs,-18)
for name,kw in (("per bin (reference-like: one bin, no band)",dict(res=0,hmin=0.5)),("main lobe only (+-2 bins)",dict(res=0)),("1/12 oct",dict(res=1)),("1/6 oct (default)",dict(res=2)),("1/3 oct",dict(res=4))):
    p=dict(sc.DEF,window=2048,thresh=-40,down=4,up=4,attack=10,release=50,lift_range=24,range=60,**kw)
    d=sc.Detector(p,fs); e,X,Ls,g=d.gains(x); m=(d.f>200)&(d.f<8000); G=g[20:][:,m]
    tstd=np.mean(np.std(G,axis=0)); fr=np.sqrt(np.mean(np.diff(G,axis=1)**2)); dt=np.sqrt(np.mean(np.diff(G,axis=0)**2))
    print(f"{name:42s}: gain std over time {tstd:.2f} dB, bin-to-bin {fr:.2f} dB, hop-to-hop {dt:.2f} dB")
print("== pre-echo: a pluck (E4) after 0.6 s of -80 dBFS white noise; Studio N 2048; output level before the onset")
n=int(1.4*fs); x=rng.standard_normal(n)*10**(-80/20); on=int(0.6*fs); s=sc.pluck(329.63,0.8,fs,seed=5); s=s/np.abs(s).max()*10**(-6/20); x[on:on+len(s)]+=s
def pre(y,D,lab):
    yy=y[D:]; seg=lambda a,b: 20*np.log10(np.sqrt(np.mean(yy[on-int(a*fs/1e3):on-int(b*fs/1e3)]**2))+1e-12)
    pk=20*np.log10(np.sqrt(np.mean(yy[on:on+int(0.01*fs)]**2)))
    print(f"{lab:48s}: RMS 0-5 ms before onset {seg(5,0)-pk:6.1f} dB, 5-20 ms {seg(20,5)-pk:6.1f}, 20-45 ms {seg(45,20)-pk:6.1f} (re first 10 ms)")
pre(x,0,"input")
for name,kw in (("Studio down 8, per bin (hmin 0, res 0)",dict(res=0,hmin=0.5,down=8)),("Studio down 8, 1/6 oct",dict(res=2,down=8)),
                ("Studio down 8 up 8, per bin",dict(res=0,hmin=0.5,down=8,up=8)),("Studio down 8 up 8, 1/6 oct",dict(res=2,down=8,up=8)),
                ("Studio down 8 up 8, 1/6 oct, N 1024",dict(res=2,down=8,up=8,window=1024))):
    kw2=dict(dict(window=2048),**kw)
    y,A=sc.run(x,fs,mode='studio',thresh=-40,attack=5,release=100,auto_gain=False,lift_range=24,range=60,return_all=True,**kw2)
    pre(y,A['D'],name)
y,A=sc.run(x,fs,mode='live',window=1024,thresh=-40,down=8,up=8,attack=5,release=100,auto_gain=False,lift_range=24,range=60,return_all=True)
pre(y,0,"Live down 8 up 8, N 1024")
