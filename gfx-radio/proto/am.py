import numpy as np, scipy.signal as sg
def spec(y,fs,f0,nh=12):
    n=len(y); w=np.blackman(n); Y=np.abs(np.fft.rfft(y*w))/np.sum(w)*2; f=np.fft.rfftfreq(n,1/fs)
    def pk(fr):
        i=int(round(fr*n/fs)); return Y[max(i-4,0):i+5].max()
    a1=pk(f0); hs=[pk(k*f0) for k in range(2,nh) if k*f0<fs/2-200]
    return a1,np.sqrt(sum(h*h for h in hs))/a1, Y, f
def rx(e,fs,delta,B=3000.,taud=None,audio=4500.):
    n=len(e); t=np.arange(n)/fs
    c=e*np.exp(-2j*np.pi*delta*t)
    c=sg.sosfilt(sg.butter(6,B,fs=fs,output='sos'),c)
    d=np.abs(c)
    if taud:
        g=np.exp(-1/(taud*fs)); o=np.empty(n); s=0.
        for i in range(n):
            s=max(d[i],s*g); o[i]=s
        d=o
    d=d-np.mean(d[n//2:])
    return sg.sosfilt(sg.butter(4,audio,fs=fs,output='sos'),d), c
fs=48000; n=fs*2; t=np.arange(n)/fs
print('--- detune, 1 kHz tone m=0.85, B=3k Butterworth-6')
for f0 in (1000.,2000.):
  for d in (0,500,1000,1500,2000,2500,3000,3500,4500,6000,9000):
    e=1+0.85*np.sin(2*np.pi*f0*t)
    y,c=rx(e,fs,d); a1,thd,_,_=spec(y[fs:],fs,f0)
    car=np.abs(np.mean(c[fs:]*np.exp(2j*np.pi*d*t[fs:])))
    print(f' f0 {f0:5.0f} delta {d:5d} Hz ({d/90:5.1f} % tune): carrier {20*np.log10(car+1e-9):6.1f} dB  fund {20*np.log10(a1+1e-9):6.1f} dB  THD {100*thd:6.1f} %')
print('--- diagonal clipping: tau_d = 53 us (x1) and x3 (drive 100), m=0.85')
for mult in (1,2,3):
  for f0 in (500.,1000.,2000.,3000.):
    e=1+0.85*np.sin(2*np.pi*f0*t)
    y,_=rx(e,fs,0,taud=mult/(2*np.pi*3000)); a1,thd,_,_=spec(y[fs:],fs,f0)
    print(f' tau x{mult} f0 {f0:5.0f}: THD {100*thd:5.1f} %  (theory onset f = {3000/mult*np.sqrt(1-.85**2)/.85:.0f} Hz)')
print('--- overmodulation aliasing at host rate: e=max(0,1+m x), f0=1231 Hz, m=2.5; in-IF inharmonic content')
for fs2,lab in ((48000,'naive 48k'),(96000,'2x naive'),(48000*16,'16x ref')):
    n2=fs2*2; t2=np.arange(n2)/fs2; f0=1231.
    e=np.maximum(0,1+2.5*np.sin(2*np.pi*f0*t2))
    if fs2!=48000: e=sg.resample_poly(e,1,fs2//48000,window=('kaiser',14))
    y,_=rx(e,48000,0); y=y[48000:]; nn=len(y)
    w=np.blackman(nn); Y=np.abs(np.fft.rfft(y*w))/np.sum(w)*2; f=np.fft.rfftfreq(nn,1/48000)
    a1=Y[int(round(f0*nn/48000))-3:int(round(f0*nn/48000))+4].max()
    mask=np.ones(len(f),bool)
    for k in range(0,20):
        mask&=np.abs(f-k*f0)>40
    mask&=f<4500
    print(f' {lab:10s}: worst non-harmonic below 4.5 kHz {20*np.log10(Y[mask].max()/a1):6.1f} dB re fund')
print('--- |c| detector aliasing, carrier on skirt (delta 3 kHz), tone 1231 Hz m .85: non-harmonic junk')
for fs2 in (44100,48000,96000):
    n2=fs2*2; t2=np.arange(n2)/fs2; f0=1231.
    e=1+0.85*np.sin(2*np.pi*f0*t2)
    y,_=rx(e,fs2,3000.); y=y[fs2:]; nn=len(y)
    w=np.blackman(nn); Y=np.abs(np.fft.rfft(y*w))/np.sum(w)*2; f=np.fft.rfftfreq(nn,1/fs2)
    a1=Y[int(round(f0*nn/fs2))-3:int(round(f0*nn/fs2))+4].max()
    mask=(f<4500)
    for k in range(0,40): mask&=np.abs(f-k*f0)>40
    print(f' fs {fs2}: worst non-harmonic {20*np.log10(Y[mask].max()/a1):6.1f} dB re fund')
