# time constants read on the computed gain at the tone's bin, timed at the frame END (when it can act):
# Studio applies it to the frame centred N/2 earlier; Live applies it from the frame end on.
import numpy as np, sc
fs=48000
n=int(3.6*fs); t=np.arange(n)/fs; a=np.where((t>=1)&(t<2.5),10**(-10/20),10**(-40/20)); x=a*np.sin(2*np.pi*1000*t)
def tc(te,g,t0,g0,g1,frac):
    tgt=g0+frac*(g1-g0); m=te>=t0; i=np.argmax((g[m]-tgt)*np.sign(g1-g0)>=0); return (te[m][i]-t0)*1e3
for W in (512,1024,2048,4096):
  for att,rel in ((1,50),(10,100),(50,300),(150,300)):
    p=dict(sc.DEF,window=W,down=4,attack=att,release=rel,knee=0)
    d=sc.Detector(p,fs); e,X,Ls,g=d.gains(x); k=int(round(1000*d.N/fs)); gk=g[:,k]; te=e/fs
    g0=0; g1=gk[(te>2.3)&(te<2.45)].mean()
    # reference: frame centre timing -> subtract N/2
    print(f"N{W} att{att:>3} rel{rel}: GR {g1:.1f}; frame-end 10/63/90%: {tc(te,gk,1.0,0,g1,.1):.0f}/{tc(te,gk,1.0,0,g1,.63):.0f}/{tc(te,gk,1.0,0,g1,.9):.0f} ms; release 63/90%: {tc(te,gk,2.5,g1,0,.63):.0f}/{tc(te,gk,2.5,g1,0,.9):.0f} ms (N/2 = {d.N/2/fs*1e3:.1f} ms)")
