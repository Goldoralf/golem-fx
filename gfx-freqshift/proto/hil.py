import numpy as np
HA=[0.6923878,0.9360654322959,0.9882295226860,0.9987488452737]
HB=[0.4021921162426,0.8561710882420,0.9722909545651,0.9952884791278]
def ap(coefs,w):
    z2=np.exp(-2j*w); H=np.ones_like(w,dtype=complex)
    for a in coefs:
        c=a*a; H*= (c - z2)/(1 - c*z2)
    return H
def image(fs,f,delayB):
    w=2*np.pi*np.array([f],float)/fs
    A=ap(HA,w); B=ap(HB,w); z1=np.exp(-1j*w)
    if delayB: I,Q=A,B*z1
    else: I,Q=A*z1,B
    # out = I cos + Q sin (pitch.rs).  input e^{jwt}: I c + Q s = (I - jQ)/2 e^{j phi} + (I + jQ)/2 e^{-j phi}
    up=abs(I-1j*Q)/2; dn=abs(I+1j*Q)/2
    return up[0],dn[0],np.degrees(np.angle(Q/I))[0]
for fs in (44100,48000):
  print('fs',fs)
  for f in (30,82,200,1000,5000,10000,15000,20000):
    u,d,ph=image(fs,f,True); u2,d2,ph2=image(fs,f,False)
    print(f'{f:6d} pitch.rs(delay B): up {20*np.log10(u):7.1f} dn {20*np.log10(d):7.1f} phase {ph:7.1f} | delay A: up {20*np.log10(u2):7.1f} dn {20*np.log10(d2):7.1f} phase {ph2:7.1f}')
