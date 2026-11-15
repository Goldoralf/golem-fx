import numpy as np, glob, os, sys, warnings
from scipy.io import wavfile
from scipy import signal
warnings.filterwarnings("ignore")
files=sorted(set(glob.glob(os.path.expanduser('~/golem/assets/.cache/*.wav'))+glob.glob(os.path.expanduser('~/golem/assets/irs/**/*.wav'),recursive=True)))
rows=[]
def load(f):
    sr,d=wavfile.read(f)
    if d.dtype.kind=='i': d=d/float(2**(8*d.dtype.itemsize-1))
    d=d.astype(float)
    if d.ndim>1: nch=d.shape[1]; d=d.mean(1)
    else: nch=1
    return sr,nch,d
def loud_gain(h,sr,lo=80,hi=6000):
    H=np.abs(np.fft.rfft(h,1<<16))**2; f=np.fft.rfftfreq(1<<16,1/sr)
    m=(f>=lo)&(f<=hi); W=1/f[m]
    return np.sqrt(W.sum()/(W*H[m]).sum())
seen=set()
for f in files:
    try: sr,nch,h=load(f)
    except Exception as e: continue
    if len(h)<32 or np.max(np.abs(h))==0: continue
    key=(len(h),round(float(np.abs(h).sum()),6))
    if key in seen: continue
    seen.add(key)
    pk=np.max(np.abs(h)); ipk=int(np.argmax(np.abs(h)))
    on=int(np.argmax(np.abs(h)>=pk*0.1))
    e=np.cumsum(h**2); tot=e[-1]
    # length to -60 dB energy tail
    tail=np.searchsorted(e,tot*(1-1e-6))/sr*1000
    def dropped(back_ms):
        s=max(0,on-int(round(back_ms*1e-3*sr))); return 10*np.log10(max(e[s-1]/tot if s>0 else 0,1e-30))
    seg=h[on:]; 
    b,a=signal.butter(2,1000/(sr/2)); lp=signal.lfilter(b,a,seg)
    n2=int(2e-3*sr); p_design=np.sign(lp[:n2].sum())
    p_peak=np.sign(h[ipk])
    # first extremum above -12 dB re peak
    idx=np.argmax(np.abs(h)>=pk*0.25); j=idx
    while j+1<len(h) and abs(h[j+1])>=abs(h[j]) and np.sign(h[j+1])==np.sign(h[j]): j+=1
    p_first=np.sign(h[j])
    rows.append(dict(f=os.path.basename(f)[:46],sr=sr,nch=nch,ms=len(h)/sr*1000,tail=tail,on=on/sr*1000,ipk=ipk/sr*1000,d25=dropped(.25),d50=dropped(.5),
        pk=20*np.log10(pk),gl=20*np.log10(loud_gain(h,sr)),pd=p_design,pp=p_peak,pf=p_first,path=f))
print(f"{'file':46s} sr ch   len_ms tail99.9999 onset peak_ms  drop.25 drop.5  peak_dB raw_loud  pol(design,peak,first)")
for r in (rows if __name__=="__main__" else []):
    print(f"{r['f']:46s} {r['sr']//1000:3d} {r['nch']} {r['ms']:8.1f} {r['tail']:8.1f} {r['on']:6.2f} {r['ipk']:6.2f} {r['d25']:7.1f} {r['d50']:7.1f} {r['pk']:6.1f} {-r['gl']:7.1f}   {int(r['pd']):+d} {int(r['pp']):+d} {int(r['pf']):+d}")
cab=[r for r in rows if __name__=="__main__" and r['tail']<250]
if __name__=="__main__":
    L=np.array([-r['gl'] for r in cab]); P=np.array([r['pk'] for r in cab])
    print(f"\n{len(rows)} distinct IRs, {len(cab)} cab-like (<250 ms). raw loudness (pink 80-6k) spread: min {L.min():.1f} max {L.max():.1f} sd {L.std():.1f} dB; median {np.median(L):.1f}")
    print(f"after Peak normalisation loudness spread: sd {np.std(L-P):.1f}, range {np.ptp(L-P):.1f} dB; median {np.median(L-P):.1f}")
    print("polarity: design vs first-extremum agree:",sum(r['pd']==r['pf'] for r in cab),"/",len(cab)," design vs peak:",sum(r['pd']==r['pp'] for r in cab)," negatives by design:",sum(r['pd']<0 for r in cab)," by first:",sum(r['pf']<0 for r in cab))
    import pickle; pickle.dump(rows,open('rows.pkl','wb'))
