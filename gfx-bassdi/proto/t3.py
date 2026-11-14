from chain import *
N=1<<16
def bins(y,k): return np.fft.rfft(y[-N:]*1.0)[k]/(N/2)
def run(x,k0,drive,gr=0,order=4,L=41.2):
    c,d=modern(x,drive,gr,0,L,order)
    C=bins(c,k0);D=bins(d,k0)
    ph=np.degrees(np.angle(D/C)); 
    return ph,20*np.log10(abs(D)/abs(C))
print('--- single sine: phase of dirt fundamental vs clean (deg), dirt/clean dB')
for f0 in (41.2,82.4,220,1000):
    k0=int(round(f0*N/fs)); n=np.arange(3*N); 
    for lev in (-30,-12,0):
        x=db(lev)*np.sin(2*PI*k0*n/N)
        print(f0,lev,[tuple(np.round(run(x,k0,dr),2)) for dr in (6,27,48)])
print('--- bass-like: harmonics with random phases; per-partial phase diff and blend loss at 50% (rms-matched)')
rng=np.random.default_rng(7)
for f0,L in ((41.2,41.2),(30.87,30.87),(82.4,41.2)):
  k0=int(round(f0*N/fs)); n=np.arange(3*N)
  for trial in range(4):
    amps=np.array([0.5,1.0,0.6,0.35,0.25,0.15,0.1,0.07])  # weak fundamental, like a real low string
    phs=rng.uniform(0,2*PI,8)
    x=sum(a*np.sin(2*PI*k0*(i+1)*n/N+p) for i,(a,p) in enumerate(zip(amps,phs))); x*=db(-12)/np.max(np.abs(x))
    for dr in (6,27,48):
        for gr in (0,-12,12):
            c,d=modern(x,dr,gr,0,L,4)
            k=np.sqrt(np.mean(c[-N:]**2)/np.mean(d[-N:]**2))
            res=[]
            for h in (1,2,3):
                C=bins(c,k0*h);D=k*bins(d,k0*h)
                res.append((round(np.degrees(np.angle(D/C)),1), round(20*np.log10(abs(D)/abs(C)),1), round(20*np.log10(abs(C+D)/(abs(C)+abs(D))),2)))
            # energy below 100Hz
            print(f0,'trial',trial,'drive',dr,'grunt',gr,'h1..3 (phase deg, dirt/clean dB, loss dB):',res)
