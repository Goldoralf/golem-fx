import numpy as np
f=np.array([100,500,1000,2000,3000,4000,5000,7000,10000.]); s=2j*np.pi*f
def taper(rot,b=81.): return (b**rot-1)/(b-1)
def par(*z): return 1/sum(1/x for x in z)
def truth(L,R,Cp,Rv,Rt,Cc,a,ZL):
    """EMF -> voltage at pedal input. Tone pot full up = Rt to ground at the pickup node."""
    Zs=R+s*L
    up=max((1-a)*Rv,1.0); lo=max(a*Rv,1e-3)
    Zw=par(lo,1/(s*Cc),ZL)                      # wiper node load
    Zp=par(1/(s*Cp),Rt,up+Zw)                   # pickup node load
    return Zp/(Zs+Zp)*Zw/(up+Zw)
def designer(L,R,Cp,Rv,Rt,Cc,a,ZL,DI):
    """README: source = DI behind (R+sL) || Cp, then the pot and cable again."""
    Zth=par(R+s*L,1/(s*Cp))
    up=max((1-a)*Rv,1.0); lo=max(a*Rv,1e-3)
    Zw=par(lo,1/(s*Cc),ZL); Zp=up+Zw
    return DI*Zp/(Zth+Zp)*Zw/(up+Zw)
db=lambda x:20*np.log10(abs(x))
SC=(2.5,6e3,100e-12,250e3,250e3,500e-12); HB=(4.5,8e3,150e-12,500e3,500e3,500e-12)
np.set_printoptions(precision=1,suppress=True,linewidth=200)
print('f       ',f)
for name,pu in (('single coil',SC),('humbucker',HB)):
    rec=truth(*pu,1.0,1e6+0*s)                  # what the interface records (vol full, 1 Mohm)
    print(name,'\n DI re EMF (recording)           ',db(rec))
    for a_rot,ZLn,ZL in ((1.0,'1M (buffer)',1e6),(1.0,'8k face',8e3),(1.0,'130k muff',130e3),(0.7,'8k face',8e3),(0.5,'8k face',8e3),(0.5,'1M (buffer)',1e6)):
        a=taper(a_rot); t=truth(*pu,a,ZL+0*s); d=designer(*pu,a,ZL+0*s,rec)
        print(' vol %3d%% (a=%.3f) load %-12s truth re DI'%(a_rot*100,a,ZLn),db(t/rec),'\n'+' '*37+'README model error',db(d/t))
# broadband numbers of 1.1
for Rv,L,R in ((250e3,2.5,6e3),(500e3,2.5,6e3)):
  for rot in (1.0,0.7,0.5):
    a=taper(rot); up=(1-a)*Rv; lo=a*Rv
    Rs=par(lo,up+R) if a<1 else R; k=lo/(lo+up+R)
    print('pot %dk rot %.0f%%: a=%.3f, source R %.1fk, open-circuit loss %.1f dB, into 8k: loss %.1f dB (extra %.1f dB vs buffered)'%(Rv/1e3,rot*100,a,Rs/1e3,-20*np.log10(k),-20*np.log10(k*8e3/(8e3+Rs)),-20*np.log10(8e3/(8e3+Rs))))
print('L/R corner into 8k: %.0f Hz (SC) %.0f Hz (HB)'%((6e3+8e3)/(2*np.pi*2.5),(16e3)/(2*np.pi*4.5)))
# inverse filter gain: EMF/DI and band-limited by 2nd-order Butterworth at 20 kHz
for name,pu in (('SC',SC),('HB',HB)):
    ff=np.array([1e3,3e3,5e3,10e3,15e3,20e3]); ss=2j*np.pi*ff
    L,R,Cp,Rv,Rt,Cc=pu; Y=ss*(Cp+Cc)+1/Rv+1/Rt+1e-6; inv=1+(R+ss*L)*Y; wc=2*np.pi*20e3
    print(name,'EMF/DI dB at',ff,':',db(inv),' with 2-pole 20 kHz limit:',db(inv/(1+1.414*ss/wc+(ss/wc)**2)), ' f_res %.0f Hz'%(1/(2*np.pi*np.sqrt(L*(Cp+Cc)))))
