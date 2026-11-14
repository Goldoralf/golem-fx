from lib import *
fs=48000
w,_=signal.freqz([1],[1],worN=np.geomspace(10,20000,400),fs=fs)
worst={}
for order in (4,2):
  for f in (80,120,200,1500,6500):
    lp,hp,ap=(lr4_parts if order==4 else lr2_parts)(f,fs)
    _,L=signal.freqz(*lp,worN=w,fs=fs);_,H=signal.freqz(*hp,worN=w,fs=fs);_,A=signal.freqz(*ap,worN=w,fs=fs)
    for g in (0,0.01,0.25,0.5,1,2,4):
      for S in (g*L+H, L+g*H):
        ratio=S/A   # must be real >=0
        m=np.abs(S)>1e-7
        e=np.max(np.abs(np.angle(ratio[m])))
        mag_err=np.max(np.abs(np.abs(S)-(np.abs(g*L)+np.abs(H) if S is None else np.abs(S))))
        worst[order]=max(worst.get(order,0),e)
    # sum = AP
    worst[(order,'sum')]=max(worst.get((order,'sum'),0),np.max(np.abs(L+H-A)))
print('max phase error rad (g*LP+HP vs AP), digital 48k:',worst)
# shelf shapes
for order in (4,2):
    lp,hp,ap=(lr4_parts if order==4 else lr2_parts)(200,fs)
    ff=np.array([50,100,141,200,283,400,800])
    _,L=signal.freqz(*lp,worN=ff,fs=fs);_,H=signal.freqz(*hp,worN=ff,fs=fs)
    for gdb in (-12,-6,12):
        print('LR%d low shelf %+d dB @200:'%(order,gdb),np.round(20*np.log10(np.abs(db(gdb)*L+H)),2))
# group delays of twins at DC
for order in (4,2):
    for f in (80,90,120,133,150,165,200,110):
        lp,hp,ap=(lr4_parts if order==4 else lr2_parts)(f,fs)
        wg,gd=signal.group_delay(ap,w=np.array([20.,41,82,f]),fs=fs)
        print('LR%d AP f=%g: gd ms at 20/41/82/fc'%(order,f),np.round(gd/fs*1000,2))
