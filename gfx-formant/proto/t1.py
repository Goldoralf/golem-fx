from live import *
np.set_printoptions(precision=1,suppress=True,linewidth=200)
dur=int(1.5*fs)
print("== A. steady /a/: achieved harmonic levels vs target (true env at f/alpha; and vs TE-of-input at f/alpha)")
for f0 in (100,220,400):
    x=tract(source(f0,dur),VOW['a'])*0.1
    for st in (-7,-4,4,7,12,-12):
        al=2**(st/12)
        y,Ls,its,Es=live(x,f0,al,ret=True)
        n0,n1=fs//2,fs//2+int(round(fs/f0*40))
        hi=harm_levels(x,f0,n0,n1); ho=harm_levels(y,f0,n0,n1)
        k=np.arange(1,len(hi)+1); fk=k*f0; sel=(fk>=200)&(fk<=5000)
        tgt=20*np.log10((f0/(fk/al))*tract_resp(VOW['a'],fk/al))   # true env at f/alpha
        d=ho-tgt; d-=np.mean(d[sel])
        # predicted by the applied L
        Lm=Ls[len(Ls)//2:].mean(0); pred=hi+np.interp(fk,np.arange(1025)*fs/NFFT,Lm)
        e2=ho-pred
        # warble: hop-to-hop std of L at harmonics
        Lh=np.array([np.interp(fk[sel],np.arange(1025)*fs/NFFT,l) for l in Ls[len(Ls)//2:]])
        clampfrac=np.mean(Ls[len(Ls)//2:]>=17.99)
        print(f"f0 {f0} st {st:+d}: err vs true env: max {np.max(abs(d[sel])):5.1f} rms {np.std(d[sel]):4.1f} dB | FIR-vs-L at harmonics max {np.max(abs(e2[sel])):4.2f} | L hop-to-hop pk-pk max {np.max(np.ptp(Lh,0)):5.2f} dB | iters mean {its.mean():.1f} | clamp frac {clampfrac:.3f} | maxL {Lm.max():.1f} minL {Lm.min():.1f}")
