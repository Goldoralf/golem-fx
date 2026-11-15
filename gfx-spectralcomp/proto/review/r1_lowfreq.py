# Review check 1: detector reading of LOW sines (below 2 bins the +/- lobes overlap): ripple of L and of the gain
import numpy as np, sc
fs=48000
print("== reading of a -20 dBFS sine at its bin over frames (hop N/8): mean / min / max  [f in bins]")
for W in (512,1024,2048,4096):
    row=[]
    for f0 in (41.2,55.0,73.4,82.4,110.0,146.8,196.0):
        t=np.arange(2*fs)/fs; x=0.1*np.sin(2*np.pi*f0*t)
        d=sc.Detector(dict(sc.DEF,window=W,hop_div=8),fs); e,X=d.frames(x); L=d.levels(X)[16:]
        k=int(round(f0*d.N/fs)); l=L[:,k]
        row.append(f"{f0:.0f}Hz[{f0*d.N/fs:.2f}] {l.mean():+.2f} ({l.min()-(-20):+.2f}/{l.max()-(-20):+.2f})")
    print(W,' | '.join(row))
def spur(y,f0,D):
    s=y[D+fs:D+3*fs]; S=np.abs(np.fft.rfft(s*np.blackman(len(s)))); f=np.fft.rfftfreq(len(s),1/fs)
    c=S[np.argmin(np.abs(f-f0))]; m=(np.abs(f-f0)>4)&(f>10)&(np.abs(f-3*f0)>4)
    i=np.argmax(S*m); return 20*np.log10(S[i]/c), f[i]
print("== consequence: sine at -12 dBFS, thresh -36, down 4, knee 6; gain ripple p-p at the bin (dB) and worst spur (dBc @ Hz) in Studio")
for W in (1024,2048,4096):
  for f0 in (41.2,82.4,110.0):
    for att,rel in ((150,300),(10,50),(1,20)):
        t=np.arange(4*fs)/fs; x=10**(-12/20)*np.sin(2*np.pi*f0*t)
        y,A=sc.run(x,fs,mode='studio',window=W,attack=att,release=rel,auto_gain=False,return_all=True)
        k=int(round(f0*A['det'].N/fs)); g=A['g'][200:,k]
        s,fq=spur(y,f0,A['D'])
        print(f"N{W} {f0:5.1f} Hz [{f0*W/fs:.2f} bins] att {att} rel {rel}: gain mean {g.mean():.2f} dB (formula -18.0), p-p {np.ptp(g):.2f} dB; worst spur {s:.1f} dBc at {fq:.1f} Hz")
