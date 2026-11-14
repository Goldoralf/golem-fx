import numpy as np
# Lotka-Volterra mode competition as drafted
def run(G, a0, rise, T, c=1.5, floor_db=-90, beta=None, Gt=None, dt=1e-3):
    G=np.array(G,float); a=np.array(a0,float); n=int(T/dt); out=np.zeros((n,len(a)))
    for i in range(n):
        if Gt is not None: G=np.array(Gt(i*dt),float)
        Gm=G.max(); u=a*a
        comp=u + c*(u.sum()-u)
        a=a*np.exp(beta*(G/Gm-comp)*dt); a=np.maximum(a,10**(floor_db/20)); out[i]=a
    return out
rise=1.2
# single mode calibration: closed form
u0=10**(-40/10); beta=np.log(3.8616*(1/u0-1))/(2*rise)   # u=0.794 at A-1dB -> u/(1-u)=3.862
o=run([1],[1e-2],rise,3,beta=beta); t=np.argmax(20*np.log10(o[:,0])>=-1)*1e-3
print("closed-form beta: A-1dB at",t,"s (rise",rise,")")
# 1. bistability: mode 0 leads, G1 slightly larger
for r in [1.05,1.2,1.4,1.6,2.0]:
    o=run([1,r],[1.0,1e-2],rise,30,beta=beta)
    print(f"G2/G1={r}: after 30 s levels dB", np.round(20*np.log10(o[-1]),1))
# 2. change of winner: G order flips by factor 2 at t=5 from floor -90
for fl in [-90,-40]:
    o=run([1,.5],[1e-2,1e-2],rise,40,beta=beta,floor_db=fl,Gt=lambda t:[1,.5] if t<5 else [.5,1])
    d=20*np.log10(o); 
    tx=np.argmax((d[:,1]>d[:,0])&(np.arange(len(d))>5000))*1e-3-5
    t1=np.argmax((d[:,1]>-1)&(np.arange(len(d))>5000))*1e-3-5
    print(f"floor {fl}: loser at flip {d[4999,1]:.0f} dB; crossover {tx:.2f} s after flip, new winner at -1 dB after {t1:.2f} s")
print("--- options")
for c in [1.1,1.2,1.5]:
  for fl in [-40,-30,-20]:
    for ratio in [1.3,2.0,4.0]:
        if ratio<=c: continue
        o=run([1,1/ratio],[1e-2,1e-2],rise,40,c=c,beta=beta,floor_db=fl,Gt=lambda t:[1,1/ratio] if t<5 else [1/ratio,1])
        d=20*np.log10(o); idx=np.arange(len(d))>5000
        tx=np.argmax((d[:,1]>d[:,0])&idx)*1e-3-5; t1=np.argmax((d[:,1]>-1)&idx)*1e-3-5
        # dip of total power during the jump
        p=10*np.log10((o[5000:]**2).sum(1)).min()
        print(f"c={c} floor={fl} ratio={ratio}: crossover {tx:.2f}s, -1dB at {t1:.2f}s (rise {rise}), min total {p:.1f} dB")
