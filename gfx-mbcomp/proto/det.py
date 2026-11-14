import numpy as np
from scipy.ndimage import maximum_filter1d
fs=48000
def opc(ms): return np.exp(-1.0/(ms*1e-3*fs))
def slmax(x,W):  # causal sliding max of |x| over W samples
    W=max(1,int(W)); a=np.abs(x)
    return maximum_filter1d(a,size=W,origin=(W-1)//2 if W>1 else 0,mode='constant')
def curve(x,t,r,w=6.0):
    d=x-t
    return np.where(2*d<-w,x,np.where(2*np.abs(d)<=w,x+(1/r-1)*(d+w/2)**2/(2*w),t+d/r))
def run(x,mode,flow,t,r,att_ms=1.0,rel_ms=250.0,char='clean'):
    n=len(x)
    if mode=='today': Wp=0.005*fs; rms_ms=10.0; post=0
    elif mode=='draft': Wp=fs/flow; rms_ms=max(10.0,2000.0/flow); post=0
    else: Wp=1.2*fs/flow; rms_ms=10.0; post=1.2*fs/flow     # review: max over 1.2 periods of the 10 ms RMS
    if char=='bus' and mode!='draft': rms_ms=5.0
    if char=='bus' and mode=='draft': rms_ms=max(5.0,2000.0/flow)
    pk=20*np.log10(slmax(x,Wp)+1e-10)-3.0103
    c=opc(rms_ms); ms=np.zeros(n); s=0.0
    for i in range(n):
        s=x[i]*x[i]+(s-x[i]*x[i])*c; ms[i]=s
    if post: ms=slmax(ms,post)
    rms=10*np.log10(ms+1e-20)
    tp=curve(pk-6,t,r)-(pk-6); tr=curve(rms,t,r)-rms
    a=opc(att_ms); rl=opc(rel_ms); g=np.zeros(n); gp=0.0; gs=0.0
    for i in range(n):
        if char=='clean':
            gp = tp[i]+(gp-tp[i])*(a if tp[i]<gp else rl)
        gs = tr[i]+(gs-tr[i])*(a if tr[i]<gs else rl)
        g[i]=min(gp,gs) if char=='clean' else gs
    return g
def thd(y,f0,n0):
    seg=y[n0:]; N=int(round(fs/f0*40)); seg=seg[:N]*np.hanning(N)
    S=np.abs(np.fft.rfft(seg)); fr=np.fft.rfftfreq(N,1/fs)
    def pk(f): i=np.argmin(np.abs(fr-f)); return S[i-3:i+4].max()
    fund=pk(f0); return fund,[20*np.log10(pk(k*f0)/fund+1e-15) for k in (2,3,4,5)]
T=6*fs; tt=np.arange(T)/fs
print('--- steady-state harmonics added (dBc: H2 H3 H4 H5), ratio 4, attack 1 ms, ~6 dB reduction; gain ripple p-p')
for name,sig in (('sine 41.2',lambda f:np.sin(2*np.pi*f*tt)),('asym note 41.2 (f+0.6 2f+0.3 3f)',lambda f:np.sin(2*np.pi*f*tt)+0.6*np.sin(2*np.pi*2*f*tt+0.9)+0.3*np.sin(2*np.pi*3*f*tt+2.0)),('sine 30.9',None)):
    f0=30.9 if '30.9' in name else 41.2
    x=(np.sin(2*np.pi*f0*tt) if sig is None else sig(f0)); x=x/np.sqrt(np.mean(x**2))*10**(-12/20)   # -12 dBFS RMS
    _,h0=thd(x,f0,3*fs)
    for char in ('clean','bus'):
        for mode in ('today','draft','review'):
            g=run(x,mode,30.0,-20.0,4.0,char=char); y=x*10**(g/20)
            _,h=thd(y,f0,3*fs); rip=g[3*fs:].max()-g[3*fs:].min()
            print('%-34s %-5s %-6s GR %.2f dB ripple %.3f dB  harm %s  (input %s)'%(name,char,mode,-g[3*fs:].mean(),rip,' '.join('%.0f'%v for v in h),' '.join('%.0f'%v for v in h0)))
print('--- step +12 dB at t=1 s (41.2 Hz sine, -24 -> -12 dBFS rms, T=-20): time to 90 % of the final reduction')
x=np.sin(2*np.pi*41.2*tt)*np.sqrt(2)*np.where(tt<1,10**(-24/20),10**(-12/20))
for char in ('clean','bus'):
    for mode in ('today','draft','review'):
        g=run(x,mode,30.0,-20.0,4.0,char=char); fin=g[int(2.5*fs):int(3*fs)].mean()
        i=np.argmax(g[fs:]<=0.9*fin); 
        # release: step down at 3 s
        print(char,mode,'final %.2f dB, 90%% after %.1f ms; overshoot %.2f dB'%(fin,i/fs*1e3,fin-g[fs:int(2.5*fs)].min()))
