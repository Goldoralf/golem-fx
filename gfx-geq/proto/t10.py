from geq import *
from t7 import mk2
from t8 import sections
w=(1,.15,.05)
for nm,lay in (('OCT',OCT),('GT',GT),('GB',GB)):
    sec=sections(lay); G=mk2(lay,sec,48000,.4,w,clamp=30); L=np.log2(np.array(lay,float)); rng=np.random.default_rng(7); its=[]
    for amp in (12,15):
        for i in range(150):
            fad=rng.uniform(-amp,amp,len(lay)); p=PchipInterpolator(L,fad); T=lambda f: p(np.clip(np.log2(f),L[0],L[-1]))
            g,n=G.solve(None,40,T=T); its.append(n)
    print(nm,len(sec),'cold-start corrections: mean %.1f max %d'%(np.mean(its),max(its)))
    # leakage: normalised B at design points >= 2 octaves from section
    far=[abs(G.B[p,m]) for m in range(len(sec)) for p in range(len(G.fp)) if abs(np.log2(G.fp[p]/sec[m]))>=2]
    far1=[abs(G.B[p,m]) for m in range(len(sec)) for p in range(len(G.fp)) if abs(np.log2(G.fp[p]/sec[m]))>=1.5]
    print('  max normalised leakage beyond 2 oct: %.3f, beyond 1.5 oct: %.3f'%(max(far),max(far1)))
lay=OCT; sec=sections(lay); G=mk2(lay,sec,48000,.4,w,clamp=30); L=np.log2(np.array(lay,float))
fad=np.zeros(11); fad[0]=-12; p=PchipInterpolator(L,fad); T=lambda f: p(np.clip(np.log2(f),L[0],L[-1])); g,n=G.solve(None,40,T=T)
print('g1=-12 alone: response at 10/20/25/31.25 Hz', G.resp(g,np.array([10,20,25,31.25])))
fad=np.zeros(11); fad[5]=12; p=PchipInterpolator(L,fad); T=lambda f: p(np.clip(np.log2(f),L[0],L[-1])); g,n=G.solve(None,40,T=T)
f=np.geomspace(20,20000,2000); r=G.resp(g,f); print('lone g6 +12: min anywhere %.2f, at 707 Hz %.2f, 500 %.2f 1k %.2f'%(r.min(),G.resp(g,np.array([707.1]))[0],G.resp(g,np.array([500.]))[0],G.resp(g,np.array([1000.]))[0]))
