import numpy as np
def inv(f,L,R,Cp,Rv,Rt,Cc=500e-12,fp=12e3,Rin=1e6):
    s=2j*np.pi*f
    Y=s*(Cp+Cc)+1/Rv+1/Rt+1/Rin
    H=1+(R+s*L)*Y
    wp=2*np.pi*fp
    B=wp**2/(s**2+np.sqrt(2)*wp*s+wp**2) if fp else 1
    return H,H*B
db=lambda x:20*np.log10(abs(x))
PU={'SC':(2.5,6e3,100e-12,250e3,250e3),'HB':(4.5,8e3,150e-12,500e3,500e3),'P90':(7,8e3,120e-12,500e3,500e3)}
f=np.array([100,1000,2000,3000,4000,6000,8000,10000,12000,16000,20000.])
for k,v in PU.items():
    H,HB=inv(f,*v)
    print(k,'unlimited',np.round(db(H),1)); print(k,'12k     ',np.round(db(HB),1))
    H20=inv(f,*v,fp=20e3)[1]; print(k,'20k     ',np.round(db(H20),1))
    # dc gain
    print('  dc',round(db(inv(np.array([20.]),*v)[0])[0],2))
# white-noise lift 20-20k (rms gain) for interface hiss, and A-weighted-ish 2-16k
ff=np.linspace(20,20000,4000)
def aw(f):
    f2=f*f
    return 12194**2*f2**2/((f2+20.6**2)*np.sqrt((f2+107.7**2)*(f2+737.9**2))*(f2+12194**2))
for k,v in PU.items():
    for fp in (8e3,10e3,12e3,20e3):
        HB=inv(ff,*v,fp=fp)[1]
        print(k,fp,'white lift dB',round(10*np.log10(np.mean(abs(HB)**2)),1),'A-w',round(10*np.log10(np.sum(abs(HB)**2*aw(ff)**2)/np.sum(aw(ff)**2)),1))
# sensitivity: true guitar differs from the guess: output error = Hinv_guess / Hinv_true
print('--- error when the guess is wrong (SC guess), dB at f')
fs=np.array([1000,2000,3000,4000,5000,6000,8000,10000.])
g=inv(fs,*PU['SC'])[0]
for name,(L,R,Cp,Rv,Rt,Cc) in {'cable 250pF':(2.5,6e3,100e-12,250e3,250e3,250e-12),'cable 900pF':(2.5,6e3,100e-12,250e3,250e3,900e-12),'L 2.0H':(2.0,6e3,100e-12,250e3,250e3,500e-12),'L 3.2H (hot SC)':(3.2,7e3,100e-12,250e3,250e3,500e-12),'true=HB':(4.5,8e3,150e-12,500e3,500e3,500e-12),'tone at 7 (R_t eff 100k)':(2.5,6e3,100e-12,250e3,100e3,500e-12)}.items():
    t=inv(fs,L,R,Cp,Rv,Rt,Cc)[0]
    print(f'{name:28s}',np.round(db(g/t),1))
# position shelf
fo=np.array([82.41,110,146.83,196,246.94,329.63]); gm=np.exp(np.mean(np.log(fo))); print('geomean',gm)
Ls=0.648
for nm,x in [('hole200',.2),('hole165',.165),('neck',.16),('mid',.1),('bridge',.041)]:
    print(nm, 'corner mean',round(2*Ls*gm/(2*np.pi*x),1),'per string',np.round(2*Ls*fo/(2*np.pi*x)))
def R(f,xe,xs,c=2*Ls*gm):
    s=2j*np.pi*f; return (s+c/xe)/(s+c/xs)
for xs in (.2,.165):
  for nm,x in [('neck',.16),('mid',.1),('bridge',.041)]:
    print('xs',xs,nm,[round(db(R(np.array([q]),x,xs))[0],1) for q in (20,50,100,170,400,840,2000,5000)])
print('bridge-neck @50', round(db(R(np.array([50.]),.041,.2)/R(np.array([50.]),.16,.2))[0],2))
# exact comb average: force/velocity = 1/|sin(k x)| ; rms and log averages above the first notch
u=np.linspace(0.001,np.pi-0.001,200001)
print('log-mean 1/|sin| dB',round(np.mean(-20*np.log10(np.sin(u))),2),' power-mean of velocity (sin^2) -> force/vel rms dB', round(-10*np.log10(np.mean(np.sin(u)**2)),2))
# true per-string ensemble vs the 1st-order shelf: energy-weighted over six strings, partial n of open strings
fgrid=np.geomspace(60,6000,25)
for nm,x in [('neck',.16),('bridge',.041)]:
    acc=[]
    for fq in fgrid:
        vals=[]
        for f0 in fo:
            c=2*Ls*f0
            # average |sin| power over a +-1/6 octave neighbourhood to mimic many notes/frets
            fl=np.geomspace(fq/1.12,fq*1.12,41)
            vals.append(np.mean(np.sin(2*np.pi*fl*x/c)**2))
        acc.append(np.mean(vals))
    acc=np.array(acc)  # mean velocity power per unit force^2 (x Z0^-2)
    exact=-10*np.log10(acc)          # force re velocity, dB
    shelf=db((2j*np.pi*fgrid+2*Ls*gm/x)/(2j*np.pi*fgrid))
    print(nm,'exact-shelf dB', np.round(exact-shelf,1))
