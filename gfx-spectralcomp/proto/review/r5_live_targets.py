# Review check 5: which target shape can 27 third-octave bells draw for a lone tone without neighbours fighting?
import numpy as np, sc
from acge import GEQ
fs=48000
def mk(lam=1e-3,c=0.4): return GEQ([f for f in sc.THIRDS],fs,c=c,wts=(1,.5,.25),clamp=40.0,lam=lam)
def solve(G,t,n=60):
    gs=np.zeros(len(G.fc)); h=[]
    for _ in range(n): gs=np.clip(gs+G.P@(t-G.resp(gs,G.fp)),-40,40)
    return gs
def shape(G,f0,depth,kind):
    x=np.log2(G.fp/f0)
    if kind=='rect 1/2 oct (as written)': t=np.where(np.abs(x)<=0.25+1e-9,depth,0.0)
    elif kind=='rect 1/2 oct + [1 2 1]': t=np.where(np.abs(x)<=0.25+1e-9,depth,0.0); t=np.convolve(np.pad(t,1,mode='edge'),[.25,.5,.25],'valid')
    elif kind=='rect 2/3 oct + [1 2 1]': t=np.where(np.abs(x)<=1/3+1e-9,depth,0.0); t=np.convolve(np.pad(t,1,mode='edge'),[.25,.5,.25],'valid')
    elif kind=='cos^2, +-1/2 oct': t=np.where(np.abs(x)<0.5,depth*np.cos(np.pi*x)**2,0.0)
    elif kind=='cos^2, +-2/3 oct': t=np.where(np.abs(x)<2/3,depth*np.cos(np.pi*x*0.75)**2,0.0)
    return t
kinds=['rect 1/2 oct (as written)','rect 1/2 oct + [1 2 1]','rect 2/3 oct + [1 2 1]','cos^2, +-1/2 oct','cos^2, +-2/3 oct']
fe=np.geomspace(30,18000,1500)
for lam in (1e-3,3e-2):
  G=mk(lam)
  print(f"=== lambda {lam}")
  for kind in kinds:
    rows=[]
    for depth in (-12,-24,+12):
      e_t=[];mxg=[];oct1=[];wid=[];opp=[]
      for f0 in (100.,150.,1000.,1060.,1122.,5000.,5300.,7100.,10000.):
        t=shape(G,f0,depth,kind); gs=solve(G,t)
        r0=G.resp(gs,np.array([f0]))[0]; e_t.append(r0-depth); mxg.append(np.abs(gs).max()); opp.append(max(0,(-np.sign(depth)*gs).max()))
        R=G.resp(gs,fe); x=np.log2(fe/f0); oct1.append(np.abs(R[np.abs(x)>=0.75]).max())
        inside=np.abs(R)>=abs(r0)/2; wid.append(np.ptp(x[inside]) if inside.any() else 0)
      rows.append(f"{depth:+d} dB: err at tone {min(e_t):+.1f}..{max(e_t):+.1f}, largest |section| {max(mxg):.0f}, largest opposing section {max(opp):.0f}, width at half depth {np.mean(wid):.2f} oct, worst beyond 3/4 oct {max(oct1):.1f} dB")
    print(f"{kind:28s} | "+'\n'+' '*31+('\n'+' '*31).join(rows))
