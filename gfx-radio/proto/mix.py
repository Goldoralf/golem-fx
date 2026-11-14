import numpy as np, scipy.signal as sg
f=np.geomspace(60,8000,4000); s=2j*np.pi*f
def an(b,a): return np.polyval(b,s)/np.polyval(a,s)
def chain(parts):
    H=np.ones_like(s)
    for z,p,k in parts:
        b,a=sg.zpk2tf(z,p,k); H*=an(b,a)
    return H
def lp2(fc,Q): w=2*np.pi*fc; return sg.tf2zpk([w*w],[1,w/Q,w*w])
def hp2(fc,Q): w=2*np.pi*fc; return sg.tf2zpk([1,0,0],[1,w/Q,w*w])
tel=[lp2(1400,1.5),hp2(250,.707),sg.butter(4,2*np.pi*280,'high',analog=True,output='zpk'),
     sg.cheby1(6,0.5,2*np.pi*3500,analog=True,output='zpk'),hp2(400,0.8),lp2(3400,1.0)]
pocket=[hp2(350,1.0),lp2(6000,0.9),hp2(80,.7)]
mega=[hp2(200,.7),hp2(500,.9),hp2(500,.7),sg.butter(4,2*np.pi*4500,analog=True,output='zpk')]
for name,ch in (('Telephone',tel),('Pocket Radio',pocket),('Megaphone',mega)):
    H=chain(ch)
    # auto-match: normalise so band rms gain ~ pink-weighted unity inside the band
    g=1/np.sqrt(np.mean(abs(H)**2)/ (np.mean(abs(H)>0.5*abs(H).max()))) ; H=H*g
    ph=np.unwrap(np.angle(H)); band=abs(H)>0.5*abs(H).max()
    print(f'{name}: wet phase swing inside its -6 dB band: {np.ptp(ph[band])/np.pi:.1f} pi')
    for mix in (0.3,0.5,0.7):
        T=(1-mix)+mix*H; ref=np.maximum(1-mix,mix*abs(H))
        dep=20*np.log10(abs(T)/ref); i=np.argmin(dep)
        print(f'   mix {int(mix*100)} %: deepest notch {dep[i]:6.1f} dB at {f[i]:5.0f} Hz (re the louder of dry / wet); notches > 6 dB: {np.sum((dep[1:-1]<dep[:-2])&(dep[1:-1]<dep[2:])&(dep[1:-1]<-6))}')
