import numpy as np, time
# 1. PXS compressor reference level
rng=np.random.default_rng(3)
for N in (256,4096,32768,262144):
    w=0.53836-0.46164*np.cos(2*np.pi*np.arange(N)/(N+1.0))
    x=rng.standard_normal(N)  # rms 1
    F=np.abs(np.fft.rfft(x*w))[:N//2]; F[0]=0
    rms=np.sqrt(np.mean(F**2))*0.1
    print(f"PXS compress: window {N}: 0.1*rms = {rms:.1f} for a 0 dBFS-RMS frame -> unity gain at {-20*np.log10(rms):+.1f} dBFS RMS; floor 1e-3 at {20*np.log10(1e-3/rms):+.1f} dBFS; max lift +60 dB at p=1")
# 2. amplitude blur vs power blur width
x=np.arange(-40000,40001)*0.001
e=np.exp(-np.abs(x)); h=np.convolve(e,e,'same'); h/=h.sum()
sd=lambda p: np.sqrt((p*x*x).sum()/p.sum())
print("4-one-pole kernel sigma (tau=1):",sd(h)," power profile sigma:",sd(h*h)," ratio",sd(h*h)/sd(h))
for n in (0.3,0.5,0.6,0.7,0.8,0.85,0.9,0.95,1.0):
    s=2400*np.log2(48000/40)/(16384*(-np.log(1-2**(-10*n*n))))
    print(f"  spread {n:.2f}: amplitude sigma {s:7.1f} c -> power-profile sigma {s*sd(h*h)/sd(h):7.1f} c")
# 3. first-touch
for mb in (48,118):
    t=time.perf_counter(); a=np.empty(mb*1024*1024,dtype=np.uint8); a[:]=1; dt=time.perf_counter()-t
    t=time.perf_counter(); a[:]=2; dt2=time.perf_counter()-t
    print(f"first touch {mb} MB: {dt*1e3:.1f} ms; rewrite {dt2*1e3:.1f} ms")
# 4. hann^2 OLA sums
N=1024;w=0.5-0.5*np.cos(2*np.pi*np.arange(N)/N)
print("sum w^2 hop N/4", sum(np.roll(w**2,k*N//4) for k in range(4))[:3], "mean w^2",np.mean(w**2))
# 5. unit counts at 2^18
print("resync example x8 maxlen 10:", 10/0.875, 10/0.875/8)
