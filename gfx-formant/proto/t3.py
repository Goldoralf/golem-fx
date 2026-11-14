from live import *
import live
dur=int(1.0*fs); fgrid=np.arange(1025)*fs/NFFT
def aw(L,m):
    return awin(L,m)
def lag(w):
    age=len(w)-1-np.arange(len(w)); return np.sum(w*w*age)/np.sum(w*w)/fs*1000
print("window scan: L (periods), fall m (periods) -> centroid lag ms, hop-to-hop envelope ptp (max/median dB), cold iters (delta 2 / delta 1)")
for f0 in (100,220,400):
    for vib in (0,50):
        x=tract(source(f0,dur,vib),VOW['a'])*0.1
        fk=np.arange(1,int(5000/f0)+1)*f0
        T=fs/f0
        for Lp,mp in ((3,0.25),(3,0.75),(3,1.0),(3,1.5),(4,1.0),(4,1.5),(2,1.0),(2.5,1.0),(2.5,1.25)):
            L=int(min(Lp*T,NFFT)); m=int(min(mp*T,L/2)); w=aw(L,m)
            Es=[];its=[];its1=[]
            for s in range(fs//2,fs//2+48*HOP,HOP):
                seg=x[s-L:s]; X=np.fft.rfft(seg*w/np.sum(w)*2,NFFT); lm=20*np.log10(abs(X)+1e-9)
                O=int(np.clip(0.5*fs/f0,24,400)); A0=pool(lm)
                V,it=te(A0,O,maxit=200); _,it1=te(A0,O,maxit=200,delta=1.0)
                Es.append(np.interp(fk,fgrid,up(V))); its.append(it); its1.append(it1)
            Es=np.array(Es); p=np.ptp(Es,0)
            if vib: p=np.ptp(Es-Es.mean(1,keepdims=True),0)
            print(f"f0 {f0} vib {vib:2d} L {Lp}T m {mp}T: lag {lag(w):5.1f} ms | ptp max {p.max():4.1f} med {np.median(p):4.1f} | iters {np.mean(its):4.1f}/{max(its)}  d1: {np.mean(its1):4.1f}/{max(its1)}")
