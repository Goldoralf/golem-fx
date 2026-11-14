# two-band velvet: side = Vlong * LP1(M) + Vshort * (M - LP1(M)); long = T, short = T/4
import numpy as np, sys
from vnd import fs,fg,AE,AT,devs,erb_bands
from vnd2 import cost2
def run(seed,T_ms=20,ratio=4,fx=1000,nl=16,ns=14,iters=9000,dec=6,minpos=24,quiet=False):
    r=np.random.default_rng(seed); L=int(T_ms*fs/1000); Ls=L//ratio
    lp=1/(1+1j*fg/fx); hp=1-lp
    def init(n,Lx):
        e=np.linspace(minpos,Lx,n+1); return np.floor(e[:-1]+r.uniform(0,1,n)*(e[1:]-e[:-1])).astype(float)
    pos=np.concatenate([init(nl,L),init(ns,Ls)]); isl=np.arange(nl+ns)<nl; sgn=r.choice([-1.,1.],nl+ns)
    Lx=np.where(isl,L,Ls)
    def build(pos,sgn):
        g=sgn*10**(-dec/20*pos/Lx)
        g[isl]/=np.sqrt(np.sum(g[isl]**2)); g[~isl]/=np.sqrt(np.sum(g[~isl]**2)); return g
    def resp(pos,g):
        E=g[None,:]*np.exp(-2j*np.pi*fg[:,None]*pos[None,:]/fs)
        return lp*E[:,isl].sum(1)+hp*E[:,~isl].sum(1)
    g=build(pos,sgn); G=resp(pos,g); c=cost2(G,dc=g[isl].sum())
    for it in range(iters):
        p2=pos.copy(); s2=sgn.copy(); k=r.integers(nl+ns)
        if r.random()<0.25: s2[k]=-s2[k]
        else:
            p2[k]=np.clip(p2[k]+np.round(r.normal(0,Lx[k]/12)),minpos,Lx[k])
            if len(np.unique(p2[isl]))<nl or len(np.unique(p2[~isl]))<ns: continue
        g2=build(p2,s2); G2=resp(p2,g2); c2=cost2(G2,dc=g2[isl].sum()); T=0.3*(1-it/iters)+1e-3
        if c2<c or r.random()<np.exp((c-c2)/T): pos,sgn,g,G,c=p2,s2,g2,G2,c2
    o=[]
    for A in (AT,AE):
        dl,dr,ds=devs(G,0.6,A); d=np.concatenate([dl,dr]); o.append((d.min(),d.max(),ds.min(),ds.max()))
    if not quiet: print(f"2band T={T_ms} short={T_ms/ratio:g}ms fx={fx} s{seed}: one-side 1/3oct [{o[0][0]:+.1f},{o[0][1]:+.1f}] ERB [{o[1][0]:+.1f},{o[1][1]:+.1f}] side level ERB [{o[1][2]:+.1f},{o[1][3]:+.1f}]")
    return pos,g,isl,o
if __name__=='__main__':
    for seed in (1,2,3,4): run(seed)
    for seed in (1,2): run(seed,ratio=8,fx=1500)
    for seed in (1,2): run(seed,T_ms=30,ratio=6,fx=1000)
