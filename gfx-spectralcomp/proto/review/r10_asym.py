# Review check 10: Live only needs an ANALYSIS window (nothing is resynthesised). Asymmetric Hann (long rise, short fall):
# reading accuracy on tones, and the timing of the gain at the frame end (m2b's measure), N 1024 hop N/8
import numpy as np, sc
fs=48000
sym=sc.hann
def asym(frac):
    def w(N):
        M=int(N*frac); a=np.arange(N-M)/(N-M); b=np.arange(M)/M
        return np.concatenate([0.5-0.5*np.cos(np.pi*a),0.5+0.5*np.cos(np.pi*b)])
    return w
n=int(2.2*fs); t=np.arange(n)/fs; a=np.where(t>=1,10**(-10/20),10**(-40/20)); xs=a*np.sin(2*np.pi*1000*t)
def tc(te,g,t0,g1,frac):
    m=te>=t0; i=np.argmax(g[m]<=frac*g1); return (te[m][i]-t0)*1e3
for name,wf,hmin in (("symmetric Hann, +-2 bins",sym,2.0),("fall N/4, +-2 bins",asym(.25),2.0),("fall N/4, +-3 bins",asym(.25),3.0),("fall N/8, +-3 bins",asym(.125),3.0),("fall N/8, +-4 bins",asym(.125),4.0)):
    sc.hann=wf
    for W in (1024,):
        p=dict(sc.DEF,window=W,hop_div=8,down=4,attack=1,release=100,knee=0,res=2.0,hmin=hmin)
        errs=[]
        for f0 in np.geomspace(80,8000,40):
            x=0.1*np.sin(2*np.pi*f0*np.arange(fs)/fs); d=sc.Detector(p,fs); e,X=d.frames(x); L=d.levels(X)[16:]; k=int(round(f0*d.N/fs)); errs+= [L[:,k].min()+20,L[:,k].max()+20]
        d=sc.Detector(p,fs); e,X,Ls,g=d.gains(xs); k=int(round(1000*d.N/fs)); gk=g[:,k]; te=(e+1)/fs; g1=gk[-20:].mean()
        w=wf(W); cen=(W-1-np.sum(np.arange(W)*w**2)/np.sum(w**2))/fs*1e3
        print(f"{name:26s} N{W}: tone reading error {min(errs):+.2f}..{max(errs):+.2f} dB (80 Hz - 8 kHz); energy centroid {cen:.1f} ms before the frame end; gain 10/63/90 % at {tc(te,gk,1.0,g1,.1):.1f} / {tc(te,gk,1.0,g1,.63):.1f} / {tc(te,gk,1.0,g1,.9):.1f} ms after the step")
sc.hann=sym
print("auto make-up, make-room-for-voice (thresh -50, down 6, res 2, N 1024): %.1f dB"%sc.auto_makeup(dict(sc.DEF,thresh=-50,down=6,window=1024),1024,fs))
print("auto make-up, fill-the-gaps (thresh -50, down 1, up 3, lift 9, guard -60, N 1024): %.1f dB"%sc.auto_makeup(dict(sc.DEF,thresh=-50,down=1,up=3,lift_range=9,guard=-60,window=1024),1024,fs))
