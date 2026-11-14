from geq import *
from t2 import dense
np.set_printoptions(precision=2,suppress=True,linewidth=220)
def gn(G,T,iters=30):
    t=T(G.fp); g=G.P@t; W=np.diag(G.w)
    for i in range(iters):
        r=t-G.resp(g,G.fp); J=np.zeros((len(G.fp),len(g)))
        for m in range(len(g)):
            h=0.01; gp=g.copy(); gp[m]+=h; J[:,m]=(G.resp(gp,G.fp)-G.resp(g,G.fp))/h
        dg=np.linalg.solve(J.T@W@J+1e-6*np.eye(len(g)),J.T@W@r); g=np.clip(g+dg,-30,30)
        if np.max(abs(dg))<1e-4: break
    return g
for dens in (False,True):
  for outer in (True,False):
    cent=OCT; G=GEQ(dense(cent) if dens else cent,48000,outer=outer)
    f=31.25*2**(np.arange(0,9*12+1)/12)
    for nm,fad in (('allup',np.full(11,12.)),('zig',12*(-1.)**np.arange(11)),('rand',np.random.default_rng(3).uniform(-12,12,11))):
        L=np.log2(np.array(cent)); p=PchipInterpolator(L,fad); T=lambda f: p(np.clip(np.log2(f),L[0],L[-1]))
        g,_=G.solve(None,8,T=T); g2=gn(G,T)
        e=G.resp(g,f)-T(f); e2=G.resp(g2,f)-T(f)
        print(f"dens={dens} outer={outer} {nm}: chord max {abs(e).max():.2f} centres {abs(G.resp(g,np.array(cent))-fad).max():.2f} | GN max {abs(e2).max():.2f} centres {abs(G.resp(g2,np.array(cent))-fad).max():.2f}")
        if nm!='rand' : print('  err/12th-oct (chord):',e[::3]); print('  gains',g)
