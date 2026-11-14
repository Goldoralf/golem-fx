import sys, numpy as np
def lab(h):
    c=np.array([int(h[i:i+2],16) for i in (1,3,5)])/255; c=np.where(c>0.04045,((c+0.055)/1.055)**2.4,c/12.92)
    M=np.array([[.4124564,.3575761,.1804375],[.2126729,.7151522,.0721750],[.0193339,.1191920,.9503041]]); x=M@c/np.array([.95047,1,1.08883])
    f=np.where(x>216/24389,np.cbrt(x),(24389/27*x+16)/116); return np.array([116*f[1]-16,500*(f[0]-f[1]),200*(f[1]-f[2])])
def de00(a,b):
    L1,a1,b1=a;L2,a2,b2=b; C1=np.hypot(a1,b1);C2=np.hypot(a2,b2);Cb=(C1+C2)/2;G=.5*(1-np.sqrt(Cb**7/(Cb**7+25**7)))
    a1p=(1+G)*a1;a2p=(1+G)*a2;C1p=np.hypot(a1p,b1);C2p=np.hypot(a2p,b2)
    h1=np.degrees(np.arctan2(b1,a1p))%360;h2=np.degrees(np.arctan2(b2,a2p))%360
    dL=L2-L1;dC=C2p-C1p;dh=h2-h1
    if dh>180:dh-=360
    if dh<-180:dh+=360
    dH=2*np.sqrt(C1p*C2p)*np.sin(np.radians(dh/2));Lb=(L1+L2)/2;Cbp=(C1p+C2p)/2
    hb=(h1+h2)/2 if abs(h1-h2)<=180 else ((h1+h2+360)/2 if h1+h2<360 else (h1+h2-360)/2)
    T=1-.17*np.cos(np.radians(hb-30))+.24*np.cos(np.radians(2*hb))+.32*np.cos(np.radians(3*hb+6))-.2*np.cos(np.radians(4*hb-63))
    SL=1+.015*(Lb-50)**2/np.sqrt(20+(Lb-50)**2);SC=1+.045*Cbp;SH=1+.015*Cbp*T
    RT=-2*np.sqrt(Cbp**7/(Cbp**7+25**7))*np.sin(np.radians(60*np.exp(-((hb-275)/25)**2)))
    return np.sqrt((dL/SL)**2+(dC/SC)**2+(dH/SH)**2+RT*dC/SC*dH/SH)
hs=set(open(sys.argv[1]).read().split())|set("#4a5a6e #e04a00 #f0a500 #00a8c0 #0058e0 #00a84f #3a28f0 #6a1fd1 #c4148c #8a3ff0 #6b7480 #72c000 #c8160e #5fb0ff #d8dde3 #e8336f #b316c0 #b06cf0".split())
for cand in ["#7a1450"]:
    r=sorted((de00(lab(cand),lab(h)),h) for h in hs if h.lower()!=cand)
    print(cand,"L*=%.0f"%lab(cand)[0],[(round(d,1),h) for d,h in r[:8]])
print("pitch L*",lab("#c4148c")[0], "reverb", lab("#6a1fd1")[0])
