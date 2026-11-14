import numpy as np
fs=48000; N=32768; nb=N//2+1
k=np.arange(nb)
def blur(P, sig, form):
    tau=np.maximum(k*(2**(sig/2400)-1),1e-9)
    a=np.exp(-1/tau); a[0]=0
    y=P.astype(float).copy()
    for _ in range(2):
        if form=='gather':
            for i in range(1,nb): y[i]=a[i]*y[i-1]+(1-a[i])*y[i]
            for i in range(nb-2,-1,-1): y[i]=a[i]*y[i+1]+(1-a[i])*y[i]
        else: # scatter (transposed): state carries energy, each bin keeps (1-a) of what arrives
            s=0.0
            for i in range(nb):
                s=s+y[i]; y[i]=(1-a[i])*s; s=a[i]*s   # leak forward with the local pole
            s=0.0
            for i in range(nb-1,-1,-1):
                s=s+y[i]; y[i]=(1-a[i])*s; s=a[i]*s
    return y
def meas(y):
    c=1200*np.log2(np.maximum(k,1e-9)*fs/N/1000.0)
    w=y/y.sum(); m=(w[1:]*c[1:]).sum(); sd=np.sqrt((w[1:]*(c[1:]-m)**2).sum())
    return m,sd
for f0 in (110.0,1000.0,5000.0):
    P=np.zeros(nb); k0=int(round(f0*N/fs)); P[k0]=1.0
    c0=1200*np.log2(k0*fs/N/1000.0)
    for sig in (20,50,200,600,1200,2400):
        for form in ('gather','scatter'):
            y=blur(P,sig,form); m,sd=meas(y)
            print(f"f0 {f0:6.0f} Hz sigma {sig:5d} c {form:7s}: energy {10*np.log10(y.sum()):+6.2f} dB  centroid shift {m-c0:+7.1f} c  measured sigma {sd:7.1f} c ({sd/sig:4.2f}x)  tau_bins {k0*(2**(sig/2400)-1):.2f}")
# flat spectrum
P=np.ones(nb)
for form in ('gather','scatter'):
    y=blur(P,600,form); print(form,'flat spectrum 600c: band 200Hz-10k min/max dB',10*np.log10(y[137:6827].min()),10*np.log10(y[137:6827].max()))
