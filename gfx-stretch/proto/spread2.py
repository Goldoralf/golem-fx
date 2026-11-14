import numpy as np
exec(open('spread.py').read().split("for f0 in")[0])
def blur2(P,sig):
    tau=np.maximum(k*np.log(2)*sig/2400,1e-9); a=np.exp(-1/tau); a[0]=0
    y=P.astype(float).copy()
    for _ in range(2):
        s=0.0
        for i in range(nb):
            s=s+y[i]; y[i]=(1-a[i])*s; s=a[i]*s
        s=0.0
        for i in range(nb-1,-1,-1):
            s=s+y[i]; y[i]=(1-a[i])*s; s=a[i]*s
    return y
for f0 in (110.0,440.0,1000.0,3000.0):
    P=np.zeros(nb); k0=int(round(f0*N/fs)); P[k0]=1.0
    c0=1200*np.log2(k0*fs/N/1000.0)
    print(f0, "  ".join(f"{sig}c: {meas(blur2(P,sig))[1]/sig:4.2f}x E{10*np.log10(blur2(P,sig).sum()):+5.2f}dB d{meas(blur2(P,sig))[0]-c0:+5.0f}c" for sig in (50,200,600,1200,2400)))
# f16
x=(0.1*np.sin(2*np.pi*440*np.arange(480000)/48000)).astype(np.float32)
rng=np.random.default_rng(0)
for nm,s in (("sine -20 dBFS",x),("sine -60 dBFS",x*0.01),("sine -90 dBFS",x*0.0003),("noise -30",(0.03*rng.standard_normal(480000)).astype(np.float32)),("guitar-like decays",(x*np.exp(-np.arange(480000)/48000.0)).astype(np.float32))):
    q=s.astype(np.float16).astype(np.float32)
    print(f"f16 {nm}: SNR {10*np.log10(np.mean(s.astype(float)**2)/np.mean((q.astype(float)-s)**2)):.1f} dB, max rel err {np.max(np.abs(q-s)/np.maximum(np.abs(s),1e-30)):.2e}")
print("f16 max", np.finfo(np.float16).max, "smallest normal", np.finfo(np.float16).tiny, 20*np.log10(np.finfo(np.float16).tiny))
