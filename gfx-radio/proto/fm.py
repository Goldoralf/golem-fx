import numpy as np, scipy.signal as sg, sys
from scipy.special import erfc
def run(fs, cnr_db, B=6000., dur=60., mod=False, seed=1, order=6):
    rng=np.random.default_rng(seed); n=int(fs*dur)
    if mod:
        t=np.arange(n)/fs
        x=0.5*np.sin(2*np.pi*1000*t)  # 1.25 kHz dev
        ph=np.cumsum(2*np.pi*2500*x/fs)
    else: ph=np.zeros(n)
    s=np.exp(1j*ph)
    sos=sg.butter(order,B,fs=fs,output='sos')
    # noise bandwidth of the IF (two-sided) measured
    w,h=sg.sosfreqz(sos,1<<16,fs=fs); Bn=2*np.trapezoid(abs(h)**2,w)
    rho=10**(cnr_db/10); sig2=1/rho*fs/Bn   # complex noise variance per sample
    nz=(rng.standard_normal(n)+1j*rng.standard_normal(n))*np.sqrt(sig2/2)
    c=sg.sosfilt(sos,s+nz)
    d=np.angle(c[1:]*np.conj(c[:-1]))
    # clicks: net winding. unwrap = cumsum(d); compare to tx phase
    tot=np.cumsum(d)-ph[1:]
    # count 2pi slips: round(tot/2pi) changes, measured on a 5 ms smoothed version
    k=int(fs*0.005); sm=np.convolve(tot,np.ones(k)/k,'same')
    lev=np.round(sm/(2*np.pi)); clicks=np.sum(np.abs(np.diff(lev)))
    # audio: discriminator /dev, 300-3000 band
    a=d*fs/(2*np.pi*2500)
    sosa=sg.butter(4,[300,3000],btype='band',fs=fs,output='sos'); y=sg.sosfilt(sosa,a)
    if mod: y=y-sg.sosfilt(sosa,x)[1:]
    r=B*2/(2*np.sqrt(3))
    return clicks/dur, r*erfc(np.sqrt(rho)), 20*np.log10(np.std(y[int(fs):])+1e-12), Bn
for fs in (44100,48000,96000,192000):
    print('fs',fs)
    for cnr in (30,15,12,10,8,6,4,2,0):
        c,th,nl,Bn=run(fs,cnr)
        cm,_,nlm,_=run(fs,cnr,mod=True)
        print(f'  CNR {cnr:3d} dB  clicks/s {c:8.2f} (mod {cm:8.2f})  Rice {th:8.2f}  audio noise {nl:6.1f} dB (mod {nlm:6.1f})  Bn {Bn:.0f}')
# no carrier: squelch-tail level
fs=48000; rng=np.random.default_rng(3); n=fs*10
sos=sg.butter(6,6000,fs=fs,output='sos')
c=sg.sosfilt(sos,rng.standard_normal(n)+1j*rng.standard_normal(n))
d=np.angle(c[1:]*np.conj(c[:-1]))*fs/(2*np.pi*2500)
de=sg.lfilter(*sg.bilinear([1],[1/(2*np.pi*300),1],fs),d)
y=sg.sosfilt(sg.butter(4,[300,3000],btype='band',fs=fs,output='sos'),de)
print('no-carrier discriminator rms dBFS raw %.1f, after de-emph+audio band %.1f, peak %.1f'%(20*np.log10(d.std()),20*np.log10(y.std()),20*np.log10(abs(y).max())))
# full-deviation 1 kHz tone through same de-emph chain for reference
t=np.arange(n)/fs; x=np.sin(2*np.pi*1000*t)
yr=sg.sosfilt(sg.butter(4,[300,3000],btype='band',fs=fs,output='sos'),sg.lfilter(*sg.bilinear([1],[1/(2*np.pi*300),1],fs),x))
print('full-dev 1k tone after same chain rms %.1f dB'%(20*np.log10(yr.std())))
