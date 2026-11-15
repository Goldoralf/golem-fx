import numpy as np, sc, ref
exec(open('m5.py').read().split("for sig_name")[0])
x=di; R={}
for name,p in PRE.items():
    pr={k:v for k,v in p.items() if k!='window'}; f,G,H=ref.ref_gains(x,fs,N=p['window'],**pr)
    Xr=np.abs(np.fft.rfft(np.stack([np.concatenate([np.zeros(p['window']-H),x])[m*H:m*H+p['window']]*sc.hann(p['window']) for m in range(len(x)//H)]),axis=1))**2
    R[name]=per_oct(f,G[10:],Xr[10:])
best=[]
for dt in (-1,0,1):
  for b in (-2.08,-1.5,-1.07):
    for ue in (1,):
      err=[]
      for name,p in PRE.items():
        q=convert(p); q['thresh']+=dt; q['slope']=0.693*p['slope']+b
        if ue==0: q['up']=p['up']
        q=dict(sc.DEF,**q); d=sc.Detector(q,fs); e,X,Ls,g=d.gains(x)
        err.append(per_oct(d.f,g[10:],np.abs(X[10:])**2)-R[name])
      err=np.array(err); best.append((np.sqrt(np.mean(err**2)),np.abs(err).max(),dt,b,ue))
best.sort()
for r in best[:6]: print("rms %.2f max %.2f  dthresh %+d  slope b %+.2f  up-conv %d"%r)
