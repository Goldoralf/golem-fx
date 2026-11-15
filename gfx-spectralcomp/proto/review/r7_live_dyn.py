# Review check 7: Live end to end, variant A (as written) vs B (proposed): lone-tone accuracy, program accuracy, step lag, tick spurs
import numpy as np, sc, live2, sys
from scipy.io import wavfile
fs=48000
rms=lambda v: 20*np.log10(np.sqrt(np.mean(v**2))+1e-30)
print("== lone sine, down 4, up 3, thresh -36, knee 6, N 1024: realised gain - formula (dB) by input level; largest section gain")
for var in 'AB':
  for f0 in (150.,1000.,1122.,5000.,7100.):
    row=[]; gm=0
    for lev in (-60,-48,-36,-24,-12,0):
        n=int(0.7*fs); x=10**(lev/20)*np.sin(2*np.pi*f0*np.arange(n)/fs)
        y,A=live2.live(x,fs,var,window=1024,down=4,up=3,attack=5,release=5,auto_gain=False)
        s=slice(int(0.5*fs),int(0.68*fs)); gr=rms(y[s])-rms(x[s])
        p=dict(sc.DEF,down=4,up=3); want=float(sc.static_gain(np.array(lev),np.array(-36.0),p))
        row.append(f"{lev}:{gr-want:+.1f}"); gm=max(gm,A['gmax'])
    print(f"  {var} {f0:6.0f} Hz  "+'  '.join(row)+f"   largest section {gm:.0f} dB")
