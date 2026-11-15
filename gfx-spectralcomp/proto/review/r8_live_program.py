# Review check 8: Live A vs B on program (realised vs target at the centres, largest section gain), step lag at the OUTPUT, tick spurs
import numpy as np, sc, live2
from scipy.io import wavfile
fs=48000
def rd(f): sr,x=wavfile.read('/home/raph/golem/library/di-refs/guitar/'+f); return (x/2**31)[:4*fs]
print("== program: thresh -40, down 3, up 2, attack 30, release 200, N 1024")
for nm in ('clean.wav','high-gain.wav'):
    x=rd(nm)
    for var,c in (('A',0.4),('B',0.4),('B',0.5)):
        y,A=live2.live(x,fs,var,window=1024,thresh=-40,down=3,up=2,attack=30,release=200,auto_gain=False,c_bell=c)
        e=A['err']; print(f"  {nm:14s} {var} c={c}: realised-vs-target at centres worst {e.max():.2f} dB, p95 {np.percentile(e,95):.2f}, median {np.median(e):.2f}; largest section gain {A['gmax']:.1f} dB; target range {A['Tm'].min():.1f}..{A['Tm'].max():.1f}; level {20*np.log10(np.std(y)/np.std(x)):+.2f} dB")
print("== step at the output: 1 kHz sine -40 -> -10 dBFS at 1.0 s, down 4, thresh -36, knee 0 (final -19.5 dB): time to 10/63/90 % of the reduction")
n=int(2.2*fs); t=np.arange(n)/fs; a=np.where(t>=1,10**(-10/20),10**(-40/20)); x=a*np.sin(2*np.pi*1000*t)
for var,W,hd,att in (('A',1024,4,1),('B',1024,4,1),('B',1024,8,1),('B',1024,16,1),('A',1024,4,10),('B',1024,8,10),('B',512,4,1),('B',512,8,1),('B',2048,4,1),('B',2048,8,1)):
    y,A=live2.live(x,fs,var,window=W,live_hop_div=hd,down=4,attack=att,release=100,knee=0,auto_gain=False)
    yy=np.sqrt(2*np.convolve(y**2,np.ones(48)/48,'same')); g=20*np.log10(np.maximum(yy,1e-12)/a)
    g1=g[int(2.0*fs):int(2.1*fs)].mean(); s=g[int(1.0*fs)+30:]
    tc=lambda fr: (np.argmax(s<=fr*g1)+30)/fs*1e3
    # overshoot energy: output peak in the first 30 ms re the settled output level
    pk=20*np.log10(np.abs(y[int(1.0*fs):int(1.03*fs)]).max()/(10**((-10+g1)/20)))
    print(f"  {var} N{W} hop N/{hd} attack {att} ms: final {g1:.1f} dB; 10/63/90 % at {tc(.1):.1f} / {tc(.63):.1f} / {tc(.9):.1f} ms; onset peak {pk:+.1f} dB over the settled level")
print("== tick spurs (m3's tremolo test), coefficients held per tick")
n=int(2.0*fs); t=np.arange(n)/fs; am=10**((-20+6*np.sin(2*np.pi*6*t))/20); x=am*np.sin(2*np.pi*1000*t)
for var,tick_ms in (('A',4/3),('B',4/3),('B',1/3)):
    y,A=live2.live(x,fs,var,window=1024,attack=10,release=50,down=4,auto_gain=False,tick_ms=tick_ms)
    tick=int(round(tick_ms*1e-3*fs)); s=y[int(0.5*fs):int(0.5*fs)+fs]; S=np.abs(np.fft.rfft(s*np.blackman(len(s)))); f=np.fft.rfftfreq(len(s),1/fs)
    c=S[np.argmin(np.abs(f-1000))]; m=(np.abs(f-1000)>40)&(f>50)
    print(f"  {var} tick {tick} samples: worst component > 40 Hz from the carrier {20*np.log10(S[m].max()/c):.1f} dBc at {f[m][np.argmax(S[m])]:.0f} Hz")
