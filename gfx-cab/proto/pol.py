import numpy as np, pickle, itertools, re
from scipy import signal
from irstats import load
rows=pickle.load(open('rows.pkl','rb'))
groups={'palmer-sm57':r'palmer-g12m65-sm57','palmer-origin':r'palmer-g12m65-origin','M25':r'^M25 ','V30':r'^V30 ','mesa412':r'412-mesa','dauntless':r'res-dauntless','chunky':r'chunkychugs','glock':'glockenklang'}
def prep(r):
    sr,_,h=load(r['path']); 
    if sr!=48000: h=signal.resample_poly(h,160,147) if sr==44100 else signal.resample_poly(h,1,2)
    return h[:24000]
def bp(h,lo=80,hi=3000,sr=48000):
    sos=signal.butter(2,[lo,hi],btype='band',fs=sr,output='sos'); return signal.sosfilt(sos,h)
def xc(a,b,maxlag=96):
    n=max(len(a),len(b)); A=np.zeros(n+2*maxlag); B=A.copy(); A[maxlag:maxlag+len(a)]=a; B[maxlag:maxlag+len(b)]=b
    r=np.array([np.dot(A, np.roll(B,l)) for l in range(-maxlag,maxlag+1)])
    return r/np.sqrt((a**2).sum()*(b**2).sum())
tot=dict(design=0,peak=0,first=0); N=0; amb=0; neg=0
for g,pat in groups.items():
    rs=[r for r in rows if re.search(pat,r['f']) or re.search(pat,r['path'].split('/')[-1])]
    hs=[bp(prep(r)) for r in rs]
    ag=dict(design=0,peak=0,first=0); n=0; lags=[]; rmax=[]
    for i,j in itertools.combinations(range(len(rs)),2):
        r=xc(hs[i],hs[j]); k=np.argmax(np.abs(r)); s=np.sign(r[k]); n+=1
        pos=r.max(); ng=-r.min()
        if min(pos,ng)/max(pos,ng)>10**(-1.5/20): amb+=1
        if s<0: neg+=1
        lags.append((k-96)/48); rmax.append(abs(r[k]))
        for key,f in (('design','pd'),('peak','pp'),('first','pf')):
            ag[key]+= (rs[i][f]*rs[j][f]==s)
    for k in tot: tot[k]+=ag[k]
    N+=n
    if n: print(f"{g:14s} {len(rs):2d} IRs {n:3d} pairs: rule agrees with correlation sign: design {ag['design']:3d} peak {ag['peak']:3d} first {ag['first']:3d} | |lag| med {np.median(np.abs(lags)):.3f} max {np.max(np.abs(lags)):.3f} ms, |r| med {np.median(rmax):.2f} min {np.min(rmax):.2f}")
print("total pairs",N,tot,"pairs with corr sign negative:",neg," ambiguous (+/- peaks within 1.5 dB):",amb)
