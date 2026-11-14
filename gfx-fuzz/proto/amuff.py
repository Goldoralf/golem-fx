import numpy as np
from scipy.signal.windows import blackmanharris
exec(open('muff.py').read().split("fs=384000.")[0])
for f0 in (1204.,5004.):
    for fs in (192000.,384000.):
        n=int(0.25*fs); t=np.arange(int(0.1*fs)+n)/fs
        c=muff(fs); y,its,bad=c.run(0.25*np.sin(2*np.pi*f0*t),1,['out'],maxit=20,cont=True)
        x=y[-n:,0]; Y=abs(np.fft.rfft(x*blackmanharris(n))); fr=np.arange(len(Y))*fs/n
        b0=int(round(f0*n/fs)); fund=Y[b0]; mask=(fr>=60)&(fr<=20000)
        for k in range(0,int(20000/f0)+2): mask&=abs(fr-k*f0)>8*fs/n
        al=Y[mask]; i=np.argmax(al)
        print('muff f0 %.0f Hz, %dx: worst alias %.1f dB re fund at %.0f Hz ; mean iters %.2f max %d >8: %.3f%% fallbacks %d unsolved %d'%(f0,fs/48000,20*np.log10(al[i]/fund),fr[mask][i],its.mean(),its.max(),100*(its>8).mean(),c.nfall,bad.sum()),flush=True)
