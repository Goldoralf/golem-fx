import numpy as np
from vnd import fs,fg,AE,AT,devs
import vnd3
# --- scaling of one 2-band design with `time`, and at 44.1k rounding
des=[vnd3.run(s,quiet=True) for s in (1,4)]
def resp2(pos,g,isl,scale,fsr=48000,fx=1000):
    p=np.round(pos*scale*fsr/48000)/fsr
    lp=1/(1+1j*fg/fx); E=g[None,:]*np.exp(-2j*np.pi*fg[:,None]*p[None,:])
    return lp*E[:,isl].sum(1)+(1-lp)*E[:,~isl].sum(1)
for pos,g,isl,o in des:
    for T in (5,10,15,20,30,40):
        for fsr in (48000,44100):
            G=resp2(pos,g,isl,T/20,fsr); dl,dr,ds=devs(G,0.6,AE); d=np.r_[dl,dr]
            dl3,dr3,_=devs(G,0.6,AT); d3=np.r_[dl3,dr3]
            print(f"time {T:2d} ms fs {fsr}: 1/3oct [{d3.min():+.1f},{d3.max():+.1f}] ERB [{d.min():+.1f},{d.max():+.1f}] side [{ds.min():+.1f},{ds.max():+.1f}]")
    print()
# variants cross-correlation (side signals, pink-weighted 100 Hz-16 kHz)
G1=resp2(*des[0][:3],1); G2=resp2(*des[1][:3],1); w=1/fg
print("variant cross-corr (pink 100-16k):", np.real(np.sum(w*G1*np.conj(G2)))/np.sqrt(np.sum(w*abs(G1)**2)*np.sum(w*abs(G2)**2)))
print("rho(M,side) pink 100-16k:", np.real(np.sum(w*G1))/np.sqrt(np.sum(w)*np.sum(w*abs(G1)**2)), " pink gain dB", 10*np.log10(np.sum(w*abs(G1)**2)/np.sum(w)))
# correlation law with rho
for nm,G in (('velvet2b',G1),('comb20',np.exp(-2j*np.pi*fg*0.02))):
    for gg in (0.3,0.6,1,1.5):
        num=np.sum(w*(1-gg*gg*abs(G)**2)); PL=np.sum(w*abs(1+gg*G)**2); PR=np.sum(w*abs(1-gg*G)**2)
        print(f"  {nm} g={gg}: r={num/np.sqrt(PL*PR):+.3f} law {(1-gg*gg)/(1+gg*gg):+.3f}")
# --- bass_keep on the side only: separation of a hard-panned source, by filter type
import scipy.signal as sg
def H(kind,x):
    s=1j*x
    b2=s*s/(s*s+np.sqrt(2)*s+1)
    return {'1st':s/(1+s),'BW2':b2,'LR4':b2*b2,'BW3':s**3/((s+1)*(s*s+s+1))}[kind]
print("\nside-only high-pass at fc: side level below, crosstalk of a hard-panned source above (dB)")
for k in ('1st','BW2','BW3','LR4'):
    lo=[20*np.log10(abs(H(k,x))) for x in (0.25,0.5,1)]
    xt=[20*np.log10(abs(1-H(k,x))/abs(1+H(k,x))) for x in (1.5,2,3,4,8)]
    lv=[20*np.log10(abs(1+H(k,x))/2) for x in (1.5,2,3,4,8)]
    print(f" {k}: side at fc/4,fc/2,fc = {lo[0]:+.1f} {lo[1]:+.1f} {lo[2]:+.1f} | crosstalk at 1.5,2,3,4,8 fc = "+" ".join(f"{v:+.1f}" for v in xt)+" | panned-ch level "+" ".join(f"{v:+.1f}" for v in lv))
# --- Classic Haas + bass_keep seam: R = M + H*a*(Md - M), L = M
fc=120; tau=0.015; f=np.linspace(20,2000,4000); x=f/fc
for k in ('LR4','BW2'):
    Hh=H(k,x); R=1+Hh*(np.exp(-2j*np.pi*f*tau)-1); Rdb=20*np.log10(abs(R))
    for lo,hi in ((60,120),(120,240),(240,480),(480,960)):
        m=(f>=lo)&(f<hi); print(f" Haas a=1 τ=15ms, {k} at {fc}: R level {lo}-{hi} Hz [{Rdb[m].min():+.1f},{Rdb[m].max():+.1f}] dB")
    # fold
    F=(1+R)/2; ideal=(1+np.exp(-2j*np.pi*f*tau))/2
    m=(f>=240); print("   fold vs ideal comb above 2fc: max dev of |F| at comb peaks dB", np.max(np.abs(20*np.log10(abs(F[m])+1e-9)-20*np.log10(abs(ideal[m])+1e-9))[abs(ideal[m])>0.5]))
