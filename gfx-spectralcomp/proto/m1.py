# Measurements 1: latency / identity, calibration, static curve (README §5 rows 1-3)
import numpy as np, sc
fs=48000
print("== Studio identity at unity gains (ratios 1), impulse latency")
for W in (512,1024,2048,4096):
    x=np.zeros(3*W*2); x[W*2]=1.0
    y,a=sc.run(x,fs,mode='studio',window=W,down=1,up=1,auto_gain=False,return_all=True)
    pk=np.argmax(np.abs(y)); D=a['D']
    rng=np.random.default_rng(0); n=rng.standard_normal(fs)*0.1
    yn=sc.run(n,fs,mode='studio',window=W,down=1,up=1,auto_gain=False)
    e=yn[D+W:]-n[W:len(n)-D]
    print(W,"latency",pk-W*2,"=N-1" if pk-W*2==W-1 else "", "null %.1f dB"%(20*np.log10(np.std(e)/np.std(n))))
print("== calibration: sine reading (dB) at bin of the sine, by N; amplitude -20 dBFS")
for W in (512,1024,2048,4096):
  for f0 in (100.,440.,1003.7,5000.):
    t=np.arange(fs)/fs; x=0.1*np.sin(2*np.pi*f0*t)
    d=sc.Detector(dict(sc.DEF,window=W),fs); e,X=d.frames(x); L=d.levels(X)[-1]
    k=int(round(f0*d.N/fs)); print(W,f0,"%.2f"%L[k],end=' | ')
  print()
print("== reference (vdH) per-bin peak reading of the same sine, ov=16: 20log10(A)+10log10(N/96)")
for W in (1024,2048,4096):
    t=np.arange(fs)/fs; x=0.1*np.sin(2*np.pi*1000*t); N=W; w=sc.hann(N)
    gc=1/((16/4)*1.5)/N; X=np.fft.rfft(x[:N]*w*np.sqrt(gc)); print(W,"%.2f"%(20*np.log10(np.abs(X).max())), "formula %.2f"%(-20+10*np.log10(N/96)))
print("== pink noise reading flat per band? (std across 100 Hz-10 kHz of time-mean level)")
rng=np.random.default_rng(1); n=len(np.arange(4*fs)); Xw=np.fft.rfft(rng.standard_normal(n)); f=np.fft.rfftfreq(n,1/fs)
Xw[1:]/=np.sqrt(f[1:]); Xw[0]=0; pk=np.fft.irfft(Xw,n); pk*=10**(-18/20)/np.std(pk)
for W in (1024,2048):
  d=sc.Detector(dict(sc.DEF,window=W),fs); e,X=d.frames(pk); L=d.levels(X)[8:].mean(0); m=(d.f>100)&(d.f<10000)
  sl=np.polyfit(np.log2(d.f[m]),L[m],1)[0]
  print(W,"mean %.1f dB, slope %.2f dB/oct, predicted band level %.1f"%(L[m].mean(),sl,-18-10*np.log10(np.log2(1000)/(2/12))))
print("== static curve: 1 kHz sine swept -80..0 dBFS, down 4 up 3, thresh -36, knee 0/6; steady-state gain vs formula")
for mode in ('studio','live'):
  for knee in (0,6):
    errs=[]
    for lev in range(-80,1,4):
        x=10**(lev/20)*np.sin(2*np.pi*1000*np.arange(int(1.2*fs))/fs)
        y,a=sc.run(x,fs,mode=mode,window=1024,down=4,up=3,knee=knee,attack=5,release=5,auto_gain=False,lift_range=24,range=60,return_all=True)
        seg=slice(int(0.8*fs),int(1.15*fs)); D=a['D']
        gm=20*np.log10(np.std(y[seg])/np.std(x[seg.start-D:seg.stop-D]))
        p=dict(sc.DEF,down=4,up=3,knee=knee,lift_range=24,range=60)
        gf=sc.static_gain(np.array([lev*1.0]),np.array([-36.0]),p)[0]
        errs.append(gm-gf)
    print(mode,"knee",knee,"max |err| %.2f dB"%np.max(np.abs(errs)), "errs:",' '.join('%.1f'%e for e in errs[::3]))
