import numpy as np, sc
src=open('r5_live_targets.py').read().split("kinds=[")[0]; exec(src)
fe=np.geomspace(30,18000,1500)
for c in (0.2,0.3,0.4,0.5):
  G=mk(1e-3,c)
  for kind in ('rect 1/2 oct + [1 2 1]','rect 2/3 oct + [1 2 1]'):
    for depth in (-12,-24):
      e_t=[];mxg=[];wid=[];rip=[]
      for f0 in (100.,150.,1000.,1060.,1122.,5000.,5300.,7100.,10000.):
        t=shape(G,f0,depth,kind); gs=solve(G,t); r0=G.resp(gs,np.array([f0]))[0]; e_t.append(r0-depth); mxg.append(np.abs(gs).max())
        R=G.resp(gs,fe); x=np.log2(fe/f0); inside=np.abs(R)>=abs(r0)/2; wid.append(np.ptp(x[inside]))
      # flat-plateau ripple: all targets = depth over 2 octaves
      t=np.where(np.abs(np.log2(G.fp/1000))<=1,depth,0.0); gs=solve(G,t); m=np.abs(np.log2(fe/1000))<0.8; rip.append(np.ptp(G.resp(gs,fe)[m]))
      print(f"c {c} {kind:24s} {depth:+d}: err at tone {min(e_t):+.1f}..{max(e_t):+.1f}, largest |section| {max(mxg):.0f}, half-depth width {np.mean(wid):.2f} oct, ripple on a 2-octave plateau {rip[0]:.2f} dB p-p")
