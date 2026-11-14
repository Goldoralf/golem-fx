from chain import *
rng=np.random.default_rng(1); N=1<<16; f=np.fft.rfftfreq(N,1/fs)
for amp in (1e-4,1e-6):
  x=np.tile(rng.standard_normal(N)*amp,3)
  for gr,at in ((0,0),(12,12),(-12,-12)):
    c,d=modern(x,6,gr,at,41.2,4)
    X=np.fft.rfft(x[2*N:]);Cc=np.fft.rfft(c[2*N:])/X;D=np.fft.rfft(d[2*N:])/X
    ph=np.degrees(np.angle(D/Cc))
    for lo,hi in ((20,100),(100,1000),(1000,5000),(5000,10000),(10000,15000)):
        m=(f>=lo)&(f<hi); print(amp,gr,at,lo,hi,'max |phase| deg',round(np.max(np.abs(ph[m])),4),'dirt level dB',round(20*np.log10(np.min(np.abs(D[m]))),1))
