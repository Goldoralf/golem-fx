import numpy as np
from scipy import signal
PI=np.pi
# ---- HIIR half-band (port of gfx-dsp oversample.rs)
def halfband_coefs(n=8,transition=0.04):
    k=np.tan((1-transition*2)*PI/4)**2
    kk=(1-k*k)**0.25; e=0.5*(1-kk)/(1+kk); e4=e**4
    q=e*(1+e4*(2+e4*(15+150*e4)))
    order=n*2+1
    out=[]
    for idx in range(n):
        c=idx+1
        i=0;j=1.0;num=0.0
        while True:
            t=q**(i*(i+1))*np.sin((2*i+1)*c*PI/order)*j; num+=t; j=-j; i+=1
            if abs(t)<=1e-100 or i>200: break
        num*=q**0.25
        i=1;j=-1.0;den=0.0
        while True:
            t=q**(i*i)*np.cos(2*i*c*PI/order)*j; den+=t; j=-j; i+=1
            if abs(t)<=1e-100 or i>200: break
        den+=0.5
        ww=num/den; w2=ww*ww
        x=np.sqrt((1-w2*k)*(1-w2/k))/(1+w2)
        out.append((1-x)/(1+x))
    return out
C=halfband_coefs()
def chain(x,coefs):
    for c in coefs: x=signal.lfilter([c,1.0],[1.0,c],x)
    return x
def up2(x):
    a=chain(x,C[0::2]); b=chain(x,C[1::2])
    y=np.empty(2*len(x)); y[0::2]=a; y[1::2]=b; return y
def down2(x):
    a=chain(x[1::2],C[0::2]); b=chain(x[0::2],C[1::2]); return 0.5*(a+b)
def up4(x): return up2(up2(x))
def down4(x): return down2(down2(x))
# ---- ADAA1
def lncosh(x):
    a=np.abs(x); return a+np.log1p(np.exp(-2*a))-np.log(2)
def adaa(x,b=0.0):
    F=lncosh(x+b)-x*np.tanh(b)
    x1=np.concatenate([[0.0],x[:-1]]); F1=np.concatenate([[lncosh(b)],F[:-1]])
    d=x-x1
    small=np.abs(d)<1e-6
    dd=np.where(small,1.0,d)
    y=np.where(small,np.tanh(0.5*(x+x1)+b)-np.tanh(b),(F-F1)/dd)
    return y
def half(x): return 0.5*(x+np.concatenate([[0.0],x[:-1]]))
# ---- LR sections via bilinear with prewarp (same transfer function as TPT SVF)
def bl(b,a,f,fs):
    wa=2*fs*np.tan(PI*f/fs)
    # analog prototype in s/wa
    n=max(len(b),len(a)); b=np.r_[np.zeros(n-len(b)),b]; a=np.r_[np.zeros(n-len(a)),a]
    bs=[c/wa**(n-1-i) for i,c in enumerate(b)]; as_=[c/wa**(n-1-i) for i,c in enumerate(a)]
    return signal.bilinear(bs,as_,fs)
R2=np.sqrt(2)
def lr4_parts(f,fs):
    den=np.polymul([1,R2,1],[1,R2,1])
    lp=bl([1.0],den,f,fs); hp=bl([1,0,0,0,0],den,f,fs); ap=bl([1,-R2,1],[1,R2,1],f,fs)
    return lp,hp,ap
def lr2_parts(f,fs):
    den=[1,2,1]
    lp=bl([1.0],den,f,fs); hpn=bl([-1,0,0],den,f,fs); ap=bl([-1,1],[1,1],f,fs)  # hpn = -HP2
    return lp,hpn,ap
def section(x,f,fs,g_lo=1.0,g_hi=1.0,order=4):
    lp,hp,ap=(lr4_parts if order==4 else lr2_parts)(f,fs)
    return g_lo*signal.lfilter(*lp,x)+g_hi*signal.lfilter(*hp,x)
def twin(x,f,fs,order=4):
    lp,hp,ap=(lr4_parts if order==4 else lr2_parts)(f,fs)
    return signal.lfilter(*ap,x)
def db(g): return 10**(g/20)
