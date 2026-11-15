# Review check 2: Studio, STEADY sine under a static cut: images at f +- k fs/H (time aliasing of a gain that is not flat over the lobe)
import numpy as np, sc
fs=48000
def worst(y,f0,D,H):
    s=y[D+fs:D+3*fs]; S=np.abs(np.fft.rfft(s*np.blackman(len(s)))); f=np.fft.rfftfreq(len(s),1/fs)
    c=S[np.argmin(np.abs(f-f0))]; w=-300; wf=0
    for k in range(1,9):
        for sg in (-1,1):
            fq=abs(f0+sg*k*fs/H)
            if fq<fs/2-50:
                m=np.abs(f-fq)<3; v=20*np.log10(S[m].max()/c)
                if v>w: w=v; wf=fq
    return w,wf
orig=sc.band_edges
def run(label,be,hop_div=8,res=2.0):
    sc.band_edges=be
    print("--",label)
    for W in (1024,2048):
        row=[]
        for f0 in (82.4,110.0,146.8,196.0,261.6,329.6,440.0,659.3,987.8,1975.5,3951.0):
            t=np.arange(4*fs)/fs; x=10**(-12/20)*np.sin(2*np.pi*f0*t)
            y,A=sc.run(x,fs,mode='studio',window=W,attack=10,release=50,auto_gain=False,hop_div=hop_div,res=res,return_all=True)
            w,wf=worst(y,f0,A['D'],A['det'].H); row.append(f"{f0:.0f}:{w:.0f}")
        print(f"  N{W} hop N/{hop_div} res {res}: worst image dBc  "+'  '.join(row))
    sc.band_edges=orig
run("design as written (band >= +-2 bins)",orig)
run("design as written, hop N/4",orig,hop_div=4)
run("design as written, res 0",orig,res=0.0)
def be3(N,fs,res_st,hmin=2.0): return orig(N,fs,res_st,4.0)
run("band >= +-4 bins",be3)
def belo(N,fs,res_st,hmin=2.0):
    lo,hi=orig(N,fs,res_st,hmin); k=np.arange(len(lo)); K0=4
    lo=np.where(k<=K0,-0.5,lo); hi=np.where(k<=K0,np.maximum(hi,K0+2.0),hi); return lo,hi
run("bins 0..4 share one band [0, 6]",belo)
