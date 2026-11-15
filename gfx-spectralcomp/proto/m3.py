import numpy as np, sc
fs=48000
print("== hop spurs: 1 kHz sine, 6 Hz AM +-6 dB around -20 dBFS, thresh -36, down 4, attack 10, release 50")
n=int(2.0*fs); t=np.arange(n)/fs; am=10**((-20+6*np.sin(2*np.pi*6*t))/20); x=am*np.sin(2*np.pi*1000*t)
def spurs(y,D,H):
    s=y[D+int(0.5*fs):D+int(0.5*fs)+fs]; S=np.abs(np.fft.rfft(s*np.blackman(len(s))));f=np.fft.rfftfreq(len(s),1/fs)
    c=S[np.argmin(np.abs(f-1000))]; worst=-200
    for k in (1,2,3):
        for sg in (-1,1):
            fq=1000+sg*k*fs/H
            if 20<fq<20000:
                m=np.abs(f-fq)<15; worst=max(worst,20*np.log10(S[m].max()/c))
    # everything outside +-40 Hz of the carrier
    m=(np.abs(f-1000)>40)&(f>50); other=20*np.log10(S[m].max()/c)
    return worst,other
for mode,hd in (('studio',4),('studio',8),('live',4)):
    for W in (1024,2048):
        y,A=sc.run(x,fs,mode=mode,window=W,hop_div=hd,attack=10,release=50,down=4,auto_gain=False,return_all=True)
        H=A['det'].H; tick=64
        w,o=spurs(y,A['D'],H if mode=='studio' else tick)
        print(f"{mode} N{W} hop N/{hd}: spurs at k*fs/{'H' if mode=='studio' else 'tick'} {w:.1f} dBc; worst component >40 Hz from carrier {o:.1f} dBc")
