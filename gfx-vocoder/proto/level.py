import numpy as np, sys, io, contextlib
from scipy.signal import lfilter, butter, sosfilt
with contextlib.redirect_stdout(io.StringIO()):
    from floor import design
    from bank import H, bankresp
fs=48000.; T=4.0; n=int(fs*T); rng=np.random.default_rng(1)
def bank(x,fc,b):
    out=[]
    for c,bb in zip(fc,b):
        a=np.exp(-2*np.pi*bb/fs+2j*np.pi*c/fs); y=x.astype(complex)
        for _ in range(4): y=lfilter([1-abs(a)],[1,-a],y)
        h0=H(np.array([c]),c,bb,fs)[0]; out.append(2*y/h0)   # analytic: |z| = peak amplitude of a sine at fc
    return np.array(out)
def follow(e,att,rel):
    ka=np.exp(-1/(att*1e-3*fs)); kr=np.exp(-1/(rel*1e-3*fs)); y=np.empty_like(e); s=0.
    for i,v in enumerate(e):
        k=ka if v>s else kr; s=k*s+(1-k)*v; y[i]=s
    return y
def pink(n):
    X=np.fft.rfft(rng.standard_normal(n)); f=np.fft.rfftfreq(n,1/fs); X[1:]/=np.sqrt(f[1:]); X[0]=0; x=np.fft.irfft(X,n); return x/np.std(x)*0.1
def saw(f0,n):
    t=np.arange(n)/fs; x=np.zeros(n)
    for h in range(1,int(20000/f0)): x+=np.sin(2*np.pi*h*f0*t+h)/h
    return x
def bandlimit(x,lo,hi): return sosfilt(butter(4,[lo,hi],'bandpass',fs=fs,output='sos'),x)
def rms(x): return 20*np.log10(np.sqrt(np.mean(x[int(fs):]**2)))
def voc(mod,car,N,flatten=1.0,att=3.,rel=40.):
    fc,b,s,w=design(100,7000,N,'erb',40.)
    f=np.geomspace(130,7000/1.3,2000); G=1/np.sqrt(np.mean(abs(bankresp(f,fc,b,fs).sum(0))**2))
    A=bank(mod,fc,b); S=bank(car,fc,b)
    M=np.array([follow(abs(a),att,rel) for a in A]); C=np.array([follow(abs(s_),5,30) for s_ in S])
    Cbar=np.sqrt(np.mean(C**2,axis=0)); Cbar=follow(Cbar,200,200)
    g=(M/np.maximum(Cbar,1e-9))*(Cbar/np.maximum(C,Cbar*0.01))**flatten
    return G*(S.real*g).sum(0), M
mod=pink(n); ref=rms(bandlimit(mod,100,7000))
for N in (8,16,24,40):
    for name,car in (('saw110',saw(110,n)),('saw330',saw(330,n)),('noise',rng.standard_normal(n))):
        for fl in (1.0,0.0):
            w,_=voc(mod,car,N,fl); print(f"N={N:2d} {name:7s} flatten {fl:.0f}: wet - modulator(100-7k) = {rms(w)-ref:+.2f} dB")
    w,_=voc(mod,mod,N,1.0); print(f"N={N:2d} Self: {rms(w)-ref:+.2f} dB;  mix50 equal-power (dry+wet)/sqrt2 vs dry: {rms((w+mod)/np.sqrt(2))-rms(mod):+.2f} dB")
# beat ripple: two harmonics in one band
t=np.arange(n)/fs
for f0 in (100,150,250):
    x=np.sin(2*np.pi*20*f0*t)+np.sin(2*np.pi*21*f0*t)
    fc,b,s,w=design(100,7000,24,'erb',40.); k=np.argmin(abs(fc-20.5*f0)); A=bank(x,fc[k:k+1],b[k:k+1])[0]
    for rel in (20,40,100):
        m=follow(abs(A),3,rel)[int(fs):]; print(f"f0 {f0} band {fc[k]:.0f}Hz (b {b[k]:.0f}) rel {rel} ms: env ripple p-p {20*np.log10(m.max()/m.min()):.2f} dB")
