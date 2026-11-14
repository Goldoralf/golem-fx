import numpy as np
f=np.logspace(np.log10(20),np.log10(20000),2000); s=2j*np.pi*f
def stack(pos,Rl=22e3,Cl=10e-9,Ch=3.9e-9,Rh=22e3,Rs=15e3,RL=100e3,P=100e3,Rm=0.0):
    # nodes: A = LP node (Rl from src, Cl(+Rm) to gnd), B = HP node (Ch from src, Rh to gnd), W = wiper; pot A-W: pos*P, W-B: (1-pos)*P
    # src node S behind Rs. pos=0 -> wiper at A (bass)
    out=[]
    for sk in s:
        Y=np.zeros((4,4),complex); I=np.zeros(4,complex)   # S,A,B,W
        def add(a,b,y):
            if a>=0: Y[a,a]+=y
            if b>=0: Y[b,b]+=y
            if a>=0 and b>=0: Y[a,b]-=y; Y[b,a]-=y
        add(0,-1,1/Rs); I[0]=1/Rs
        add(0,1,1/Rl); add(1,-1,1/(Rm+1/(sk*Cl))); add(0,2,sk*Ch); add(2,-1,1/Rh)
        add(1,3,1/max(pos*P,1.)); add(3,2,1/max((1-pos)*P,1.)); add(3,-1,1/RL)
        out.append(np.linalg.solve(Y,I)[3])
    return 20*np.log10(abs(np.array(out)))
at=lambda H,x: H[np.argmin(abs(f-x))]
for name,kw in (('README assumed (22k/10n, 3.9n/22k)',{}),('39k/10n, 4n/22k (widely cited NYC values)',dict(Rl=39e3,Ch=4e-9))):
    H=stack(.5); m=f>150; i=np.argmin(H[m]); print(name,': noon dip %.1f dB below 100 Hz level at %.0f Hz'%(at(H,100)-H[m][i],f[m][i]))
    Hb=stack(0.); Ht=stack(1.); print('   bass end 100/1k/5k: %.1f %.1f %.1f | treble end: %.1f %.1f %.1f'%(at(Hb,100),at(Hb,1e3),at(Hb,5e3),at(Ht,100),at(Ht,1e3),at(Ht,5e3)))
    H=stack(.5,Rm=22e3,**kw) if kw else stack(.5,Rm=22e3); i=np.argmin(H[m]); print('   mids 100%% (22k in series with the LP cap): dip %.1f dB at %.0f Hz'%(at(H,100)-H[m][i],f[m][i]))
    H=stack(.5,**kw); i=np.argmin(H[m]); print('   (this variant) noon dip %.1f dB at %.0f Hz'%(at(H,100)-H[m][i],f[m][i]))
