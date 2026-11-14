import numpy as np, scipy.signal as sg
def motor(u,fs,f_s=650.,Qms=4.,Qts=0.8,beta=0.4,kappa=2.,k3=0.,b1=0.,k1=0.,iters=2,bform='poly',g=1.0):
    w=2*np.pi*f_s; h=1/fs
    cd=(1/Qts-1/Qms)/w        # damping coefficient so that small-signal Q = Qts
    def bf(x):
        if bform=='poly': return 1-beta*x*x+b1*x
        return 1/(1+beta*x*x)+0*x
    def acc(x,v,uu):
        b=bf(x); k=1+kappa*x*x+k1*x+k3*x**4
        return w*w*(b*(uu-cd*b*v)-k*x)-(w/Qms)*v     # note cd*w: electrical damping w*c*b^2*v
    x=v=0.; a=0.; out=np.empty(len(u)); xs=np.empty(len(u)); up=0.
    for n,uu in enumerate(u*g):
        a0=acc(x,v,up)
        xn=x+h*v; vn=v+h*a0
        for _ in range(iters):
            a1=acc(xn,vn,uu)
            vn=v+h/2*(a0+a1); xn=x+h/2*(v+vn)
        x,v=xn,vn; up=uu
        out[n]=acc(x,v,uu)/(w*w); xs[n]=x
        if not np.isfinite(x) or abs(x)>1e6: return None,None
    return out,xs
fs=96000
# small-signal vs analytic HP
print('small-signal vs analytic 2nd-order HP (fs 96k, 2 iterations)')
for f_s,Q in ((400,0.6),(650,1.2),(700,1.0),(120,0.8)):
    worst=0
    for f in (50,100,200,f_s,1000,2000,5000,10000,15000):
        n=int(fs*0.3); t=np.arange(n)/fs; u=1e-3*np.sin(2*np.pi*f*t)
        y,_=motor(u,fs,f_s=f_s,Qts=Q); y=y[n//2:]; tt=t[n//2:]
        amp=2*abs(np.mean(y*np.exp(-2j*np.pi*f*tt)))/1e-3
        s=1j*f/f_s; H=abs(s*s/(s*s+s/Q+1))
        worst=max(worst,abs(20*np.log10(amp/H)))
    print(f'  fs_spk {f_s} Q {Q}: worst deviation {worst:.3f} dB')
# stability at high drive
print('stability: 20 Hz full-scale square, drive gain g (x of Xmax scale), f_s=700')
n=int(fs*0.5); t=np.arange(n)/fs; sq=np.sign(np.sin(2*np.pi*20*t))
for bform in ('poly','rational'):
  for k3 in (0.,5.,50.):
    for g in (1.,3.,10.,31.6):
      for it in (2,4):
        y,xs=motor(sq,fs,f_s=700.,Qts=0.8,beta=0.5,kappa=3.,k3=k3,iters=it,bform=bform,g=g)
        if y is None: print(f'  {bform:8s} k3 {k3:4.0f} g {g:5.1f} it {it}: DIVERGED'); continue
        print(f'  {bform:8s} k3 {k3:4.0f} g {g:5.1f} it {it}: max|x| {abs(xs).max():6.2f}  max|p| {abs(y).max():9.1f}')
