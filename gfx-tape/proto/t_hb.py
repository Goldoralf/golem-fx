import numpy as np, math
def coefs(n, tr):
    k = math.tan((1-tr*2)*math.pi/4)**2; ks = (1-k*k)**0.25; e = 0.5*(1-ks)/(1+ks); e4 = e**4
    q = e*(1+e4*(2+e4*(15+150*e4))); order = 2*n+1; out = []
    for i in range(n):
        c = i+1
        num = sum(((-1)**j)*q**(j*(j+1))*math.sin((2*j+1)*c*math.pi/order) for j in range(0, 12))*q**0.25
        den = sum(((-1)**j)*q**(j*j)*math.cos(2*j*c*math.pi/order) for j in range(1, 12)) + 0.5
        ww = num/den; w2 = ww*ww; x = math.sqrt((1-w2*k)*(1-w2/k))/(1+w2); out.append((1-x)/(1+x))
    return out
c = coefs(8, 0.04)
def H(w):  # halfband lowpass at rate 2fs, w = rad/sample at the high rate
    z2 = np.exp(-2j*w); A = np.ones_like(z2); B = np.exp(-1j*w)
    for i, ci in enumerate(c):
        ap = (ci + z2)/(1 + ci*z2)
        if i % 2 == 0: A = A*ap
        else: B = B*ap
    return 0.5*(A+B)
f = np.array([100, 1000, 5000, 10000, 14000, 16000, 18000, 20000.])
for stages, fs in ((2, 48000), (3, 48000)):
    ph = np.zeros(len(f))
    for s in range(stages):
        ph += 2*np.unwrap(np.angle(H(2*np.pi*np.linspace(1e-6, 1, 20001)[:, None]*f[None, :]/(fs*2**(s+1))), axis=0)[-1] - 0)  # up + down
    gd0 = -ph[0]/(2*np.pi*f[0])  # LF group delay (s)
    D = round(gd0*fs)
    res = -ph - 2*np.pi*f*D/fs   # residual phase lag vs integer-delayed dry
    print(f"{stages} stages: LF delay {gd0*fs:.2f} samples -> dry delayed {D}; residual phase (deg) / 50% mix level (dB):")
    print("   ", [(int(ff), round(float(np.degrees(r)) % 360, 0), round(float(20*np.log10(abs(0.5+0.5*np.exp(-1j*r))+1e-9)), 1)) for ff, r in zip(f, res)])
