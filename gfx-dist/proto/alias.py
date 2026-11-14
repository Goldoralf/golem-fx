import numpy as np, subprocess, sys, os
def run(osf, f0, amp, rf, sr, gbw, dio, rpot=0, wave=0):
    env=dict(os.environ, TOL="1e-10")
    subprocess.run(["./rat3",str(osf),str(f0),str(amp),str(rf),str(sr),str(gbw),str(dio),"30","1.0",str(wave),"o.bin",str(rpot)],env=env,stderr=subprocess.DEVNULL,check=True)
    d=np.fromfile("o.bin").reshape(-1,2); n=48000*osf
    return d[:n,0], d[:n,1]
def alias(y, f0):
    X=np.abs(np.fft.rfft(y))/len(y)*2
    fund=X[f0]; b=np.arange(20, 20001); m=b[(b%f0!=0)]
    k=m[np.argmax(X[m])]
    return 20*np.log10(X[k]/fund), k
for name,sr,gbw in [("LM308",0.3,1e6),("4558",1.7,3e6),("TL072",13,3e6)]:
    for dn,dio in [("Si",0),("LED",1),("None",2)]:
        for f0 in (2003,4999):
            r=[]
            for osf in (8,16,32):
                y,_=run(osf,f0,0.25,100000,sr,gbw,dio)
                a,k=alias(y,f0); r.append(f"{osf}x {a:6.1f} dB @{k} Hz")
            print(f"{name:6s} {dn:4s} f0={f0}: "+" | ".join(r), flush=True)
