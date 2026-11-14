import numpy as np
# single-op-amp gyrator graphic EQ: H = (1 + R sum x_i Y_i) / (1 + R sum (1-x_i) Y_i), Y_i = BP_i/(r+rho_i), rho = x(1-x)Rp
def net(f,fc,x,R,r,Rp,Qmax):
    s=1j*f; num=1+0j*f; den=1+0j*f
    for c,xi in zip(fc,x):
        rho=xi*(1-xi)*Rp; Q=Qmax*r/(r+rho); w=s/c; bp=(w/Q)/(w*w+w/Q+1)
        num=num+R*xi/(r+rho)*bp; den=den+R*(1-xi)/(r+rho)*bp
    return 20*np.log10(abs(num/den))
def lone_gain(x,R,r,Rp): rho=x*(1-x)*Rp; return 20*np.log10((r+rho+R*x)/(r+rho+R*(1-x)))
def x_for(db,R,r,Rp):
    lo,hi=0.0,1.0
    for _ in range(60):
        m=(lo+hi)/2
        if lone_gain(m,R,r,Rp)<db: lo=m
        else: hi=m
    return m
fc=np.array([100,200,400,800,1600,3200,6400.]); f=np.geomspace(50,12800,400)
for Rp_r in (5,20,50):
  for Qmax in (1.5,2.5):
    r=1.0; R=(10**(15/20)-1)*r; Rp=Rp_r*r
    x15=x_for(15,R,r,Rp); x6=x_for(6,R,r,Rp)
    up=net(f,fc,[x15]*7,R,r,Rp,Qmax); mid=(f>200)&(f<3200)
    up6=net(f,fc,[x6]*7,R,r,Rp,Qmax)
    lone=net(f,fc,[.5,.5,.5,x15,.5,.5,.5],R,r,Rp,Qmax); lone6=net(f,fc,[.5,.5,.5,x6,.5,.5,.5],R,r,Rp,Qmax)
    i4=np.argmin(abs(f-400)); 
    # naive cascade of the same lone curves
    casc=sum(net(f,fc,[x15 if j==i else .5 for j in range(7)],R,r,Rp,Qmax) for i in range(7))
    casc6=sum(net(f,fc,[x6 if j==i else .5 for j in range(7)],R,r,Rp,Qmax) for i in range(7))
    print(f"Rp/r {Rp_r:2d} Qmax {Qmax}: all at +15 -> network {up[mid].min():.1f}..{up[mid].max():.1f} dB, dB-sum of lone curves {casc[mid].min():.1f}..{casc[mid].max():.1f} | all at +6 -> network {up6[mid].min():.1f}..{up6[mid].max():.1f}, dB-sum {casc6[mid].min():.1f}..{casc6[mid].max():.1f} | lone +15 leaks {lone[i4]:.1f} dB an octave away; lone +6 leaks {lone6[i4]:.1f}")
