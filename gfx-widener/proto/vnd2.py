import numpy as np, sys, vnd
from vnd import *
def cost2(G,g=0.6,dc=0.0):
    dl,dr,ds=devs(G,g,AE); d=np.concatenate([dl,dr])
    # colour term relative to the band's own power (pure 2g<Re> term) + side-level flatness
    return np.max(np.abs(d))+0.5*np.sqrt(np.mean(d**2))+3.0*np.max(np.abs(ds))+4*abs(dc)
vnd.cost=cost2
if __name__=='__main__':
    T=float(sys.argv[1]); 
    best={}
    for dec in (0,12,24):
        for seed in (1,2,3,4):
            p,gn=vnd.design(seed,T_ms=T,decay_db=dec,iters=8000); o=show(f"flat-side T={T:g} dec={dec} s{seed}",p,gn)
            if dec not in best or max(abs(o[1][0]),abs(o[1][1]))<best[dec][0]: best[dec]=(max(abs(o[1][0]),abs(o[1][1])),p,gn)
    np.save(f'best_T{int(T)}.npy',np.array([np.concatenate([best[d][1],best[d][2]]) for d in best]))
    for d in best:
        _,p,gn=best[d]; G=resp(p,gn); dl,dr,ds=devs(G,0.6,AE); b=erb_bands()
        print('dec',d,' band fc: L dev / R dev / side level dB')
        for i in range(0,14): print(f"  {np.mean(b[i]):6.0f} Hz  {dl[i]:+.1f} {dr[i]:+.1f}  side {10*np.log10(AE[i]@np.abs(G)**2):+.1f}")
