import numpy as np, io, contextlib
with contextlib.redirect_stdout(io.StringIO()):
    import level as L
    from floor import design
    from bank import bankresp
np.seterr(all='ignore')
fs=L.fs; n=L.n
def voc(mod,car,N,f,rel=40.):
    fc,b,s,w=design(100,7000,N,'erb',40.)
    fr=np.geomspace(130,7000/1.3,2000); G=1/np.sqrt(np.mean(abs(bankresp(fr,fc,b,fs).sum(0))**2))
    A=L.bank(mod,fc,b); S=L.bank(car,fc,b)
    M=np.array([L.follow(abs(a),3,rel) for a in A]); C=np.array([L.follow(abs(s_),5,30) for s_ in S])
    Cbar=L.follow(np.sqrt(np.mean(C**2,axis=0)),200,200)
    num=L.follow((M**2).sum(0),1000,1000); den=L.follow((M**2*(C/np.maximum(Cbar,1e-9))**(2*(1-f))).sum(0),1000,1000)
    rho=np.clip(np.sqrt(num/np.maximum(den,1e-12)),0.25,4)
    g=(M/np.maximum(Cbar,1e-9))*(Cbar/np.maximum(C,Cbar*0.01))**f
    return rho*G*(S.real*g).sum(0)
mod=L.pink(n); ref=L.rms(L.bandlimit(mod,100,7000)); rng=np.random.default_rng(3)
cars=(('saw110',L.saw(110,n)),('saw330',L.saw(330,n)),('noise',rng.standard_normal(n)))
for N in (8,24,40):
    for name,car in cars:
        print(N,name,' '.join(f"f={f:.1f}:{L.rms(voc(mod,car,N,f))-ref:+.2f}" for f in (1.0,0.5,0.0)))
print('release dependence, N=24 saw110 f=1:',' '.join(f"rel{r}:{L.rms(voc(mod,cars[0][1],24,1.0,r))-ref:+.2f}" for r in (20,40,120,400)))
