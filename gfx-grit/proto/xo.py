import numpy as np
def b2(s,w0,Q): return s*s/w0**2 + s/(Q*w0) + 1
def lr(s,w0,order):
    Qs=[1/np.sqrt(2)] if order==4 else [0.5411961,1.3065630]
    D=1
    for Q in Qs: D=D*b2(s,w0,Q)
    lp=1/D**2; hp=(s/w0)**order/D**2 ; ap=np.conj(D)/D
    return lp,hp,ap
def tree(f,fc,order,fs=None):
    if fs is None: s=2j*np.pi*f; w=[2*np.pi*x for x in fc]
    else:
        s=1j*np.tan(np.pi*f/fs); w=[np.tan(np.pi*x/fs) for x in fc]
    L=[lr(s,x,order) for x in w]
    b0=L[0][0]*L[1][2]*L[2][2]; b1=L[0][1]*L[1][0]*L[2][2]; b2_=L[0][1]*L[1][1]*L[2][0]; b3=L[0][1]*L[1][1]*L[2][1]
    return [b0,b1,b2_,b3], L[0][2]*L[1][2]*L[2][2]
f=np.logspace(np.log10(5),np.log10(20000),4000)
for order in (4,8):
  for fc in ([112.47,632.46,3556.6],[112.47,129.1,148.2],[23,26.4,30.3],[13200,15150,17400]):
    for fs in (None,96000):
        B,AP=tree(f,fc,order,fs); S=sum(B)
        mag=20*np.log10(abs(S)); err=abs(S-AP).max()
        # phase equality between bands where both significant
        ph=max(np.abs(np.angle(B[i]/AP))[abs(B[i])>1e-6].max() for i in range(4))
        ph=[min(x,abs(np.pi-x)) for x in [ph]][0]
        phs=np.unwrap(np.angle(S)); gd=-np.gradient(phs,2*np.pi*f)*1000
        print(order,fc,fs,"mag dev %.2e dB"%abs(mag).max(),"sum-AP %.1e"%err,"band phase dev rad %.1e"%ph,
              "gd ms @5Hz %.2f 50Hz %.2f 100 %.2f 200 %.2f 1k %.3f 4k %.3f 8k %.3f max %.2f at %.0f"%tuple([np.interp(x,f,gd) for x in (5,50,100,200,1000,4000,8000)]+[gd.max(),f[gd.argmax()]]))
