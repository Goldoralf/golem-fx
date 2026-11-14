import sys; sys.path.insert(0,'.')
import proto
from proto import *
from scipy.signal import lfilter

def perband2(x, wave='square', step_erb=0.5, f_shape=600, nmax=15, fcap=8000, purity=False, order=2):
    fcs=band_centres(step_erb=step_erb); out=np.zeros(len(x))
    for fc in fcs:
        bw=step_erb*erb(fc)
        z=cbank(x,fc,bw,order); mag=np.abs(z)+1e-12; u=z/mag
        if fc>f_shape:
            out+=z.real; continue
        ns=range(1,nmax+1,2) if wave=='square' else range(1,nmax+1)
        acc=np.zeros(len(x)); un=np.ones(len(x),complex); nset=set(ns)
        for n in range(1,nmax+1):
            un=un*u
            if n in nset and n*fc<fcap: acc+=un.real/n
        if purity:
            inst=np.angle(u[1:]*np.conj(u[:-1]))*FS/(2*np.pi); inst=np.concatenate([[fc],inst])
            k=np.exp(-1/(0.01*FS))
            m=lfilter([1-k],[1,-k],inst); v=lfilter([1-k],[1,-k],(inst-m)**2)
            w=np.exp(-v/(0.25*bw)**2)
            out+=mag*(w*acc+(1-w)*u.real)
        else:
            out+=mag*acc
    return out

print(f"{'input':28s} {'fuzz':>6s} " + ' '.join(f'{s:>9s}' for s in ['sq<300','sq<600','sq<1200','sq all','sq<600 p','saw<600 p']))
for name,notes in CHORDS.items():
    x=chord(notes)
    r=[imd_db(fuzz(x),notes)]
    for fs_,p,w in [(300,False,'square'),(600,False,'square'),(1200,False,'square'),(9e9,False,'square'),(600,True,'square'),(600,True,'saw')]:
        r.append(imd_db(perband2(x,w,f_shape=fs_,purity=p),notes))
    print(f"{name:28s} "+' '.join(f'{v:9.1f}' for v in r))
