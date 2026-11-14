import numpy as np
fs=48000; n=fs//2; t=np.arange(n)/fs
def taper(p): return np.where(p<.5, p*.2, .1+(p-.5)*1.8)
def gain(p): return 1+100e3*taper(p)/43.4
for a,b,name in [(0.1,0.3,"10->30 %"),(0.4,0.6,"40->60 %"),(0.6,0.9,"60->90 %")]:
    # triangle sweep, 100 ms each way (a pedal rocking), clean small signal
    ph=(t*5)%1; tri=np.where(ph<.5,ph*2,2-ph*2); p=a+(b-a)*tri
    x=np.sin(2*np.pi*1000*t)
    for hold in (16,4,1):
        ps=p[(np.arange(n)//hold)*hold]
        e=(gain(ps)-gain(p))*x; y=gain(p)*x
        w=np.hanning(n); E=np.abs(np.fft.rfft(e*w)); Y=np.abs(np.fft.rfft(y*w))
        print(f"{name} hold {hold:2d}: staircase error peak {20*np.log10(E.max()/Y.max()):6.1f} dB re signal at {np.argmax(E)*fs/n:.0f} Hz; rms {20*np.log10(np.std(e)/np.std(y)):6.1f} dB")
