import numpy as np
def prisma_zc(x,v):
    T=v**10
    y=(x-np.clip(x,-T,T))/(1-T)
    a=np.abs(y)
    p=3/(1-v*0.98)-3
    return np.where(a<1, y*np.power(1-np.power(1-np.minimum(a,1),12.5),p), y)
x=np.logspace(-6,0,6001)
print("v   T_dBFS  level where gain=-6dB (dBFS)  gain(dB) for -18dBFS-peak, -30, -40 input; RMS out/in for -18 dBFS sine")
t=np.arange(4096)/4096*2*np.pi
tab=[]
for v in (0.05,0.1,0.2,0.23,0.3,0.4,0.45,0.5,0.6,0.7,0.8,0.9,0.95):
    g=prisma_zc(x,v)/x
    i=np.argmax(g>=0.5); L6=20*np.log10(x[i])
    s=10**(-18/20)*np.sin(t); r=np.sqrt(np.mean(prisma_zc(s,v)**2)/np.mean(s**2))
    gs=[20*np.log10(max(np.interp(10**(L/20),x,g),1e-9)) for L in (-18,-30,-40)]
    print("%.2f %7.1f %7.1f   %6.1f %6.1f %6.1f   %6.1f dB"%(v,200*np.log10(v),L6,*gs,20*np.log10(r+1e-12)))
    tab.append((v,L6))
tab=np.array(tab)
# fit L6 vs v
print(np.polyfit(tab[:,0],tab[:,1],1), np.polyfit(tab[:,0],tab[:,1],2))
# our law: dead zone T dBFS hard (subtractive): level where gain=-6 dB is 2T => T+6 dB
# level comp sanity: bit crush Tread & zero-cross on -18 dBFS sine
s=10**(-18/20)*np.sin(t)
for bits in (16,12,8,6,5,4,3.5,3,2.5,2,1):
    st=2.0**(1-bits); y=np.round(s/st)*st
    print("bits",bits,"out/in RMS dB %.1f"%(20*np.log10(np.sqrt(np.mean(y**2)/np.mean(s**2))+1e-12)))
for T in (-60,-40,-30,-24,-20,-18,-12):
    Tl=10**(T/20); y=np.sign(s)*np.maximum(np.abs(s)-Tl,0)
    print("zc T",T,"out/in RMS dB %.1f"%(20*np.log10(np.sqrt(np.mean(y**2)/np.mean(s**2))+1e-12)))
# tanh makeup and small-signal gain
for d in (0,12,24,36,48):
    g=10**(d/20); y=np.tanh(g*s); c=np.sqrt(np.mean(s**2)/np.mean(y**2))
    print("tanh",d,"dB makeup %.1f dB small-signal net %.1f dB; out for -6 dBFS in: %.1f dB re in"%(20*np.log10(c),d+20*np.log10(c), 20*np.log10(c*np.sqrt(np.mean(np.tanh(g*s*4)**2)/np.mean((4*s)**2)))))
# heavy e=0.01 knee
