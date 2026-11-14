from live import *
np.set_printoptions(precision=1,suppress=True,linewidth=220)
dur=int(1.0*fs); fgrid=np.arange(1025)*fs/NFFT
for f0 in (100,220,400):
    x=tract(source(f0,dur),VOW['a'])*0.1
    fk=np.arange(1,int(5000/f0)+1)*f0
    true=20*np.log10((f0/fk)*tract_resp(VOW['a'],fk))
    hl=harm_levels(x,f0,fs//2,fs//2+int(round(fs/f0*40)))[:len(fk)]
    print(f"f0 {f0}: harmonic levels vs analytic (should be const offset): ptp {np.ptp(hl-true):.2f}")
    for name,kw in (("asym 3T",dict()),("sym hann 3T",dict(sym=True)),("asym 2048",dict(Lf=2048)),("sym hann 4T",dict(sym=True,Lf=4*fs/f0))):
        Es=[];its=[]
        for s in range(fs//2,fs//2+40*HOP,HOP):
            lm=logspec(x,s,kw.get('Lf',3*fs/f0),sym=kw.get('sym',False))
            O=int(np.clip(0.5*fs/f0,24,400))
            V,it=te(pool(lm),O,maxit=200); Es.append(np.interp(fk,fgrid,up(V))); its.append(it)
        Es=np.array(Es); d=Es-hl
        print(f"   {name:12s}: E at harmonics minus harmonic level: mean {d.mean():5.1f} dB, across-harm ptp of mean {np.ptp(d.mean(0)):4.1f}, hop-to-hop ptp max {np.ptp(Es,0).max():4.1f} median {np.median(np.ptp(Es,0)):4.1f} dB, iters to 2 dB cold: mean {np.mean(its):.0f} max {max(its)}")
