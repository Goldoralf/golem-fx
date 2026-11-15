# Measurements 2: time constants, hop spurs, musical noise, pre-echo (README §5)
import numpy as np, sc
fs=48000
def env_gain(y,x,D,win=96):
    yy=np.convolve(y**2,np.ones(win)/win,'same'); xx=np.convolve(np.concatenate([np.zeros(D),x])[:len(x)]**2,np.ones(win)/win,'same')
    return 10*np.log10(np.maximum(yy,1e-20)/np.maximum(xx,1e-20))
print("== time constants: 1 kHz sine -40 -> -10 dBFS step at 1.0 s, back at 2.5 s; down 4, thresh -36 (gain 0 -> -19.5 dB)")
for mode in ('studio','live'):
  for W in (1024,2048):
    for att,rel in ((10,100),(50,300),(150,300)):
      n=int(3.6*fs); t=np.arange(n)/fs; a=np.where((t>=1)&(t<2.5),10**(-10/20),10**(-40/20)); x=a*np.sin(2*np.pi*1000*t)
      y,A=sc.run(x,fs,mode=mode,window=W,down=4,attack=att,release=rel,auto_gain=False,knee=0,return_all=True)
      D=A['D']; yy=np.sqrt(2*np.convolve(y**2,np.ones(48)/48,'same')); ad=np.concatenate([np.zeros(D),a])[:n]
      g=20*np.log10(np.maximum(yy,1e-12)/np.maximum(ad,1e-12)); g[(np.abs(t-1-D/fs)<0.003)|(np.abs(t-2.5-D/fs)<0.003)]=np.nan
      g=np.where(np.isnan(g),np.interp(np.arange(n),np.flatnonzero(~np.isnan(g)),g[~np.isnan(g)]),g)
      def cross(t0,frac,sign):
          g0=g[int((t0-0.05)*fs)+D]; g1=g[int((t0+1.2 if t0<2 else t0+1.0)*fs)+D] if t0<2 else g[-int(0.05*fs)]
          target=g0+frac*(g1-g0); seg=g[int(t0*fs)+D-int(0.2*fs):]
          i=np.argmax((seg-target)*sign>=0); return (i-int(0.2*fs))/fs*1e3
      # attack: gain falls
      s1=g[int(1.0*fs)+D-int(0.3*fs):int(2.4*fs)+D]; g0=s1[:int(0.25*fs)].mean(); g1=s1[-int(0.05*fs):].mean()
      def tcross(s,g0,g1,frac):
          tgt=g0+frac*(g1-g0); i=np.argmax((s-tgt)*np.sign(g1-g0)>=0); return (i-int(0.3*fs))/fs*1e3
      a63=tcross(s1,g0,g1,0.63); a90=tcross(s1,g0,g1,0.9); pre=tcross(s1,g0,g1,0.1)
      s2=g[int(2.5*fs)+D-int(0.3*fs):]; r63=tcross(s2,g1,g0,0.63); r90=tcross(s2,g1,g0,0.9)
      print(f"{mode:6s} N{W} att{att} rel{rel}: GR {g1-g0:.1f} dB; attack 10/63/90% at {pre:.0f}/{a63:.0f}/{a90:.0f} ms; release 63/90% {r63:.0f}/{r90:.0f} ms")
