import numpy as np
# 1. droop of two 2-tap averages and a correction ((1+a)/(1+a z^-1))^2
for fsi in (96000,192000):
    f=np.array([1000,5000,10000,15000,18000,20000.]); w=2*np.pi*f/fsi
    d=np.cos(w/2)**2
    print(fsi,"droop dB",np.round(20*np.log10(d),2))
    fg=np.linspace(20,20000,2000); wg=2*np.pi*fg/fsi; z=np.exp(1j*wg)
    best=None
    for a in np.linspace(0.05,0.95,181):
        H=((1+1/z)/2*(1+a)/(1+a/z))**2
        e=abs(20*np.log10(abs(H))).max()
        if best is None or e<best[0]: best=(e,a)
    for a in (best[1],0.5,0.6,0.7,0.8,1.0):
        if a==1.0: continue
        H=((1+1/z)/2*(1+a)/(1+a/z))**2
        print("  a=%.3f max dev to 20k %.3f dB, corr gain at Nyq %.1f dB, at 0.75 Nyq %.1f"%(a,abs(20*np.log10(abs(H))).max(),40*np.log10((1+a)/(1-a)),40*np.log10(abs((1+a)/(1+a*np.exp(-1j*0.75*np.pi))))))
# mismatch without alignment: half / one inner sample between two equal paths at mix 50%
for fsi in (96000,192000):
    for d in (0.5,1.0):
        f=np.array([5000,10000,20000.]); ph=2*np.pi*f*d/fsi
        print(fsi,d,"sum loss dB",np.round(20*np.log10(np.cos(ph/2)),2))
# 2. aliasing: shaper at inner rate, ADAA1, measure non-harmonic comps below 20k (pre-decimation, ideal decimator)
def adaa(x,f,F):
    y=np.empty_like(x); x1=0.;F1=F(0.)
    Fx=F(x)
    xp=np.concatenate([[0.],x[:-1]]); Fp=np.concatenate([[F(0.)],Fx[:-1]])
    d=x-xp
    small=np.abs(d)<1e-6
    y=np.where(small,f(0.5*(x+xp)),(Fx-Fp)/np.where(small,1,d))
    return y
def lncosh(x):
    a=np.abs(x); return a+np.log1p(np.exp(-2*a))-np.log(2)
tanh=(np.tanh,lncosh)
clip=(lambda x:np.clip(x,-1,1), lambda x:np.where(np.abs(x)<=1,0.5*x*x,np.abs(x)-0.5))
def tri(x):
    p=np.mod(x+1,4)-1  # in [-1,3)
    return np.where(p<=1,p,2-p)
def Ftri(x):
    # antiderivative of triangle fold period 4: periodic part mean zero
    p=np.mod(x+1,4)-1
    return np.where(p<=1,0.5*p*p,0.5-0.5*(p-2)**2+0.5) -0.5 +0*x  # check below
# numeric check of Ftri derivative
xx=np.linspace(-9,9,200001); num=np.gradient(Ftri(xx),xx); assert np.abs(num-tri(xx))[5:-5].max()<1e-3, np.abs(num-tri(xx)).max()
fold=(tri,Ftri)
sfold=(lambda x:np.sin(np.pi/2*x), lambda x:-2/np.pi*np.cos(np.pi/2*x))
N=1<<16
def test(name,sh,f0,drive_db,os,use_adaa=True,amp=0.5):
    fsi=48000*os; k=round(f0/fsi*N); 
    if k%2==0:k+=1
    f0=k*fsi/N
    n=np.arange(N+8); x=amp*10**(drive_db/20)*np.sin(2*np.pi*k*n/N)
    y=adaa(x,*sh) if use_adaa else sh[0](x)
    y=y[8:]; Y=np.abs(np.fft.rfft(y*np.hanning(N)))/(N/4)
    bins=np.arange(len(Y)); 
    harm=np.zeros(len(Y),bool)
    for h in range(1,int(fsi/2/f0)+1):
        harm[max(0,h*k-3):h*k+4]=True
    band=bins<int(20000/fsi*N)
    al=Y[band&~harm]; 
    return 20*np.log10(al.max()+1e-30), 20*np.log10(np.sqrt((al**2).sum()/1.5)+1e-30), 20*np.log10(Y[k])
for name,sh in (("tanh",tanh),("clip",clip),("fold",fold),("sfold",sfold)):
    for drv in (12,24,36,48):
        row=[]
        for f0 in (2000,5000,9000):
            for os in (2,4):
                pk,tot,fund=test(name,sh,f0,drv,os)
                row.append("%dk %dx: pk %.0f (fund %.0f)"%(f0/1000,os,pk,fund))
        print(name,drv,"dB |"," | ".join(row))
print("no ADAA clip 24 dB 5k 2x/4x",test("clip",clip,5000,24,2,False)[0],test("clip",clip,5000,24,4,False)[0])
