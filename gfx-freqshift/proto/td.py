import numpy as np
HA=[0.6923878,0.9360654322959,0.9882295226860,0.9987488452737]
HB=[0.4021921162426,0.8561710882420,0.9722909545651,0.9952884791278]
def casc(x,co):
    y=x.copy()
    for a in co:
        c=a*a; o=np.zeros_like(y)
        for n in range(len(y)):
            o[n]=c*(y[n]+(o[n-2] if n>=2 else 0))-(y[n-2] if n>=2 else 0)
        y=o
    return y
fs=48000;N=fs*2;n=np.arange(N)
for f in (82,1000,5000,15000):
    x=np.sin(2*np.pi*f*n/fs)
    i=casc(x,HA); q=casc(x,HB); qd=np.concatenate([[0],q[:-1]])
    ph=2*np.pi*((n+1)*100/fs)
    y=i*np.cos(ph)+qd*np.sin(ph)
    seg=y[fs:]*np.blackman(fs); S=np.abs(np.fft.rfft(seg))
    print(f, 'at f+100: %.1f dB, at f-100: %.1f dB'%(20*np.log10(S[f+100]/S.max()),20*np.log10(S[f-100]/S.max())))
