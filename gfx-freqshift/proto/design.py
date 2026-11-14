import numpy as np, mpmath as mp
def hiir(n, tbw):
    k=np.tan((1-tbw*2)*np.pi/4)**2
    kk=(1-k*k)**0.25; e=0.5*(1-kk)/(1+kk); e4=e**4
    q=e*(1+e4*(2+e4*(15+150*e4)))
    return coefs(n,k,q), q
def exact(n,tbw):
    mp.mp.dps=40
    k=mp.tan((1-tbw*2)*mp.pi/4)**2
    q=mp.exp(-mp.pi*mp.ellipk(1-k*k)/mp.ellipk(k*k))
    return coefs(n,float(k),float(q),terms=200), float(q)
def coefs(n,k,q,terms=None):
    order=2*n+1; out=[]
    for idx in range(n):
        c=idx+1
        num=0;i=0;j=1
        while True:
            t=q**(i*(i+1))*np.sin((2*i+1)*c*np.pi/order)*j
            num+=t; j=-j; i+=1
            if (terms is None and abs(t)<1e-100) or (terms and i>terms) or i>2000: break
        num*=q**0.25
        den=0;i=1;j=-1
        while True:
            t=q**(i*i)*np.cos(2*i*c*np.pi/order)*j
            den+=t; j=-j; i+=1
            if (terms is None and abs(t)<1e-100) or (terms and i>terms) or i>2000: break
        den+=0.5
        ww=(num/den)**2
        x=np.sqrt((1-ww*k)*(1-ww/k))/(1+ww)
        out.append((1-x)/(1+x))
    return out
def resp(co,fs,f):
    w=2*np.pi*f/fs; z2=np.exp(-2j*w)
    A=np.ones_like(w,dtype=complex);B=np.ones_like(w,dtype=complex)
    for c in co[0::2]: A*=(c-z2)/(1-c*z2)
    for c in co[1::2]: B*=(c-z2)/(1-c*z2)
    return A,B*np.exp(-1j*w)
def worst(co,fs,lo=15.0):
    f=np.concatenate([np.geomspace(lo,1000,40000),np.linspace(1000,fs/2-1000,40000),fs/2-np.geomspace(1000,lo,40000)])
    A,B=resp(co,fs,f)
    im=np.minimum(abs(A+1j*B),abs(A-1j*B))/2
    return 20*np.log10(im.max()), f[im.argmax()]
import sys
for fs in (44100,48000,88200,96000,176400,192000):
    for n in (16,20,24):
        tb=30.0/fs
        c1,q1=hiir(n,tb); c2,q2=exact(n,tb)
        print(fs,n//2,'+',n//2,'series q=%.4f worst %.1f dB @%.0f | exact q=%.4f worst %.1f dB | formula %.1f'%(q1,*worst(c1,fs),q2,worst(c2,fs)[0],-10*np.log10(4*q2**((2*n+1)/2))*-1 if False else 10*np.log10(4*q2**((2*n+1)/2))))
c,_=exact(16,30/48000)
print('48k coefs A',c[0::2]); print('B(delayed)',c[1::2])
# group delay at 48k
fs=48000
for f in (41,82,1000,5000):
    d=0.01
    A1,_=resp(c,fs,np.array([f-d])); A2,_=resp(c,fs,np.array([f+d]))
    gd=-(np.angle(A2/A1))/(2*np.pi*2*d)
    print(f,'gd path A ms',gd[0]*1000)
# wider low edge options at 192k
for lo in (15,20,30):
    for n in (16,20,24):
        c,_=exact(n,2*lo/192000); print('192k lo',lo,n,'%.1f'%worst(c,192000,lo)[0])
for lo in (20,30):
    for fs in (44100,48000,96000):
        c,_=exact(16,2*lo/fs); print(fs,'lo',lo,'%.1f'%worst(c,fs,lo)[0])
