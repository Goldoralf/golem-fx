import numpy as np, sc, sys
fs=48000
for f0 in (1000.,150.,5000.,7100.):
  errs=[]
  for lev in range(-80,1,4):
    x=10**(lev/20)*np.sin(2*np.pi*f0*np.arange(int(1.0*fs))/fs)
    y,a=sc.run(x,fs,mode='live',window=1024,down=4,up=3,knee=6,attack=5,release=5,auto_gain=False,lift_range=24,range=60,return_all=True)
    seg=slice(int(0.7*fs),int(0.95*fs))
    gm=20*np.log10(np.std(y[seg])/np.std(x[seg]))
    gf=sc.static_gain(np.array([lev*1.0]),np.array([-36.0+0*np.log2(f0/420)]),dict(sc.DEF,down=4,up=3,knee=6,lift_range=24,range=60))[0]
    errs.append(gm-gf)
  print(f0,"max |err| %.2f"%np.max(np.abs(errs)),' '.join('%d:%.2f'%(l,e) for l,e in zip(range(-80,1,4),errs)))
