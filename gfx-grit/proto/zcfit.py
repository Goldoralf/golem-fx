import numpy as np
def prisma_gain(x,v):
    T=v**10
    y=(x-np.clip(x,-T,T))/(1-T); a=np.abs(y); p=3/(1-v*0.98)-3
    return np.where(a<1, y*np.power(1-np.power(1-np.minimum(a,1),12.5),p), y)/x
x=np.logspace(-7,0,14001)
vs=np.linspace(0.02,0.98,97); L=[]
for v in vs:
    g=prisma_gain(x,v); L.append(20*np.log10(x[np.argmax(g>=0.5)]))
L=np.array(L); T=L-6
for v in (0.02,0.05,0.1,0.23,0.5,0.775,0.9,0.98): print(v, round(float(np.interp(v,vs,T)),1))
# candidate closed forms for T(v) (ours, dBFS)
m=(vs>=0.05)
for deg in (1,2,3):
    c=np.polyfit(vs[m],T[m],deg); print(deg,np.round(c,2),"max err %.2f dB"%np.abs(np.polyval(c,vs[m])-T[m]).max())
# log form: T = a + b*log10(v) + c*v
A=np.vstack([np.ones(m.sum()),np.log10(vs[m]),vs[m]]).T; c,*_=np.linalg.lstsq(A,T[m],rcond=None); print("a+b log10 v + c v",np.round(c,2),"max err %.2f"%np.abs(A@c-T[m]).max())
