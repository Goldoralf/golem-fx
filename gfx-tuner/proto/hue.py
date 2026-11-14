import numpy as np, colorsys
def lab(h):
    r=np.array([int(h[i:i+2],16)/255 for i in (1,3,5)]); l=np.where(r<=0.04045,r/12.92,((r+0.055)/1.055)**2.4)
    M=np.array([[0.4124,0.3576,0.1805],[0.2126,0.7152,0.0722],[0.0193,0.1192,0.9505]]); x=M@l/np.array([0.95047,1,1.08883])
    f=np.where(x>0.008856,np.cbrt(x),7.787*x+16/116); return np.array([116*f[1]-16,500*(f[0]-f[1]),200*(f[1]-f[2])])
def de00(a,b):
    L1,a1,b1=a;L2,a2,b2=b;C1=np.hypot(a1,b1);C2=np.hypot(a2,b2);Cb=(C1+C2)/2;G=0.5*(1-np.sqrt(Cb**7/(Cb**7+25**7)))
    a1p=(1+G)*a1;a2p=(1+G)*a2;C1p=np.hypot(a1p,b1);C2p=np.hypot(a2p,b2);h1=np.degrees(np.arctan2(b1,a1p))%360;h2=np.degrees(np.arctan2(b2,a2p))%360
    dL=L2-L1;dC=C2p-C1p;dh=h2-h1;dh=dh-360 if dh>180 else dh+360 if dh<-180 else dh;dH=2*np.sqrt(C1p*C2p)*np.sin(np.radians(dh)/2)
    Lb=(L1+L2)/2;Cbp=(C1p+C2p)/2;hb=(h1+h2)/2 if abs(h1-h2)<=180 else (h1+h2+360)/2
    T=1-0.17*np.cos(np.radians(hb-30))+0.24*np.cos(np.radians(2*hb))+0.32*np.cos(np.radians(3*hb+6))-0.2*np.cos(np.radians(4*hb-63))
    SL=1+0.015*(Lb-50)**2/np.sqrt(20+(Lb-50)**2);SC=1+0.045*Cbp;SH=1+0.015*Cbp*T;RT=-2*np.sqrt(Cbp**7/(Cbp**7+25**7))*np.sin(np.radians(60*np.exp(-((hb-275)/25)**2)))
    return np.sqrt((dL/SL)**2+(dC/SC)**2+(dH/SH)**2+RT*dC/SC*dH/SH)
cols={'glitch lime':'#72c000','modulation':'#00a84f','utility amber':'#f0a500','filter':'#00a8c0','widener ice':'#c0e0f8','ringmod brass':'#c9b000','tape champagne':'#d8c8a0','gapmeter':'#6b7480','dynamics':'#4a5a6e'}
for cand in ('#23bf18','#1fb82a','#14b83c','#2fc91e','#18c232'):
    r,g,b=[int(cand[i:i+2],16)/255 for i in (1,3,5)]; h,l,s=colorsys.rgb_to_hls(r,g,b)
    print(cand,f"HSL {h*360:.0f} {s*100:.0f}% {l*100:.0f}%  L*={lab(cand)[0]:.0f}",' '.join(f"{k}: dE76 {np.linalg.norm(lab(cand)-lab(v)):.0f} dE00 {de00(lab(cand),lab(v)):.0f} |" for k,v in cols.items()))
for k,v in cols.items():
    r,g,b=[int(v[i:i+2],16)/255 for i in (1,3,5)]; h,l,s=colorsys.rgb_to_hls(r,g,b); print(k,v,f"hue {h*360:.0f} L*={lab(v)[0]:.0f}")
print('glitch vs modulation dE00',de00(lab('#72c000'),lab('#00a84f')))
