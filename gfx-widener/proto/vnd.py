# velvet side-generator optimisation for the M/S widener: L = M(1+g*V), R = M(1-g*V)
import numpy as np, sys, json
fs=48000
fg=np.arange(100,16500,12.5)
def erb_bands(flo=200,fhi=16000):
    out=[];fc=flo
    while fc<=fhi:
        bw=24.7*(4.37*fc/1000+1); out.append((fc-bw/2,fc+bw/2)); fc+=bw/2
    return out
def third_bands(flo=200,fhi=16000):
    fc=1000*2**(np.arange(-7,13)/3); fc=fc[(fc>=flo)&(fc<=fhi)]
    return [(c*2**(-1/6),c*2**(1/6)) for c in fc]
def avgmat(b): 
    A=np.zeros((len(b),len(fg)))
    for i,(lo,hi) in enumerate(b):
        m=(fg>=lo)&(fg<hi); A[i,m]=1/m.sum()
    return A
AE=avgmat(erb_bands()); AT=avgmat(third_bands())
def resp(pos,gain): return (gain[None,:]*np.exp(-2j*np.pi*fg[:,None]*pos[None,:]/fs)).sum(1)
def devs(G,g,A):
    n=10*np.log10(1+g*g)
    return 10*np.log10(A@np.abs(1+g*G)**2)-n, 10*np.log10(A@np.abs(1-g*G)**2)-n, 10*np.log10(A@((1+g*g*np.abs(G)**2)/(1+g*g)))
def cost(G,g=0.6,dc=0.0):
    dl,dr,ds=devs(G,g,AE); d=np.concatenate([dl,dr])
    return np.max(np.abs(d))+0.5*np.sqrt(np.mean(d**2))+0.5*np.max(np.abs(ds))+4*abs(dc)
def env(pos,L,decay_db): return 10**(-decay_db/20*pos/L)
def design(seed,T_ms=20,npulse=30,decay_db=12,iters=6000,minpos=24):
    r=np.random.default_rng(seed); L=int(T_ms*fs/1000)
    edges=np.linspace(minpos,L,npulse+1)
    pos=np.floor(edges[:-1]+r.uniform(0,1,npulse)*(edges[1:]-edges[:-1])).astype(float)
    sgn=r.choice([-1.,1.],npulse)
    def build(pos,sgn):
        g=sgn*env(pos,L,decay_db); return g/np.sqrt(np.sum(g**2))
    gain=build(pos,sgn); G=resp(pos,gain); c=cost(G,dc=gain.sum()); T0=0.3
    for it in range(iters):
        p2=pos.copy(); s2=sgn.copy(); k=r.integers(npulse)
        if r.random()<0.25: s2[k]=-s2[k]
        else:
            p2[k]=np.clip(p2[k]+np.round(r.normal(0,L/npulse)),minpos,L)
            if len(np.unique(p2))<npulse: continue
        g2=build(p2,s2); G2=resp(p2,g2); c2=cost(G2,dc=g2.sum()); T=T0*(1-it/iters)+1e-3
        if c2<c or r.random()<np.exp((c-c2)/T): pos,sgn,gain,G,c=p2,s2,g2,G2,c2
    return pos,gain
def show(name,pos,gain,g=0.6):
    G=resp(pos,gain); o=[]
    for A in (AT,AE):
        dl,dr,ds=devs(G,g,A); d=np.concatenate([dl,dr]); o.append((d.min(),d.max(),ds.min(),ds.max()))
    # time stats: energy centroid and 90% energy time
    e=gain**2; idx=np.argsort(pos); ce=np.cumsum(e[idx]); t90=pos[idx][np.searchsorted(ce,0.9)]/fs*1000
    print(f"{name:30s} one-side 1/3oct [{o[0][0]:+.1f},{o[0][1]:+.1f}] ERB [{o[1][0]:+.1f},{o[1][1]:+.1f}] L+R pow ERB [{o[1][2]:+.1f},{o[1][3]:+.1f}] dc={gain.sum():+.3f} centroid={np.sum(pos*e)/fs*1000:.1f}ms t90={t90:.1f}ms maxtap={np.max(np.abs(gain)):.2f}")
    return o
if __name__=='__main__':
    T=float(sys.argv[1]) if len(sys.argv)>1 else 20
    for dec in (0,12,30):
        for seed in (1,2,3):
            p,gn=design(seed,T_ms=T,decay_db=dec); show(f"opt T={T:g} dec={dec} s{seed}",p,gn)
