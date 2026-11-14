import numpy as np
from bank import erb, bankresp, H
def design(lo,hi,n,spacing,bmin=40.,wfix=None):
    # density 1/max(s*scale(f), dmin); find s so that integral = n-1
    f=np.geomspace(lo,hi,20001)
    sc=erb(f) if spacing=='erb' else f*(132.6/1000*0+1)  # log: scale(f)=f (s = ln ratio)
    def cum(s,dmin):
        d=np.maximum(s*sc,dmin); c=np.concatenate([[0],np.cumsum(np.diff(f)*0.5*(1/d[1:]+1/d[:-1]))]); return c,d
    # w law from s_eq (ERB units at 1 kHz)
    def wlaw(s): 
        seq=s if spacing=='erb' else s*1000/132.6
        return float(np.clip(0.93-0.085*seq,0.55,0.9))
    a,bb=1e-4,50.
    for _ in range(60):
        s=np.sqrt(a*bb); w=wfix or wlaw(s); c,d=cum(s,bmin/w)
        if c[-1]>n-1: a=s
        else: bb=s
    w=wfix or wlaw(s); c,d=cum(s,bmin/w)
    fc=np.interp(np.arange(n)*c[-1]/(n-1),c,f); dl=np.interp(fc,f,d)
    return fc,w*dl,s,w
def stat(lo,hi,n,spacing,bmin,fs=48000.,wfix=None):
    fc,b,s,w=design(lo,hi,n,spacing,bmin,wfix)
    f=np.geomspace(lo*1.3,hi/1.3,4000); R=bankresp(f,fc,b,fs); T=20*np.log10(abs(R.sum(0)))
    k=n//2; nb=20*np.log10(abs(H(np.array([fc[k]]),fc[k+1],b[k+1],fs)/1)[0])
    gd=4/(2*np.pi*b)*1e3
    i=lambda x: np.argmin(abs(fc-x))
    return f"N={n:2d} {spacing} bmin={bmin:4.0f} s={s:.2f} w={w:.2f} ripple {T.max()-T.min():5.2f} mean {T.mean():+.1f} gd: low {gd[0]:.1f} 500Hz {gd[i(500)]:.1f} 1.5k {gd[i(1500)]:.2f} 3k {gd[i(3000)]:.2f} top {gd[-1]:.2f} ms; bands<300Hz {int((fc<300).sum())} >1k {int((fc>1000).sum())}"
for sp in ('erb','log'):
    for n in (8,11,12,16,20,24,32,40):
        for bm in (0.01,40.):
            print(stat(100,7000,n,sp,bm))
print('ranges')
for a in ((50,7000,24,'erb'),(50,12000,40,'erb'),(500,2000,40,'erb'),(100,12000,16,'log'),(50,2000,40,'log'),(200,5080,16,'log'),(100,7000,24,'erb')):
    print(a, stat(*a,40.))
for fs in (44100.,96000.):
    print(fs, stat(100,7000,24,'erb',40.,fs), stat(100,7000,40,'erb',40.,fs))
