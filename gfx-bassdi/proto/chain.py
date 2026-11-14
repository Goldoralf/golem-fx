from lib import *
fs=48000
def modern(x,drive_db=6,grunt=0,attack=0,L=41.2,order=4,b=0.25,stage2=12,ret_clean=True):
    # pre sections at base rate
    pre=[(2.9*L,'lo',-6),(1500,'hi',3),(4.85*L,'lo',grunt),(1500,'hi',attack)]
    d=x.copy(); c=x.copy()
    for f,k,g in pre:
        d=section(d,f,fs,db(g) if k=='lo' else 1,db(g) if k=='hi' else 1,order); c=twin(c,f,fs,order)
    d=up4(d); c=up4(c)
    d=adaa(d*db(drive_db),b)
    c=half(c)
    d=section(d,3000,4*fs,1,db(-6),order); c=twin(c,3000,4*fs,order)
    d=adaa(d*db(stage2),0.0); c=half(c)
    d=down4(d); c=down4(c)
    d=section(d,6500,fs,1,0,order); c=twin(c,6500,fs,order)
    return c,d
def tf(x,y,n0=8192):
    X=np.fft.rfft(x[n0:]);Y=np.fft.rfft(y[n0:]); return Y/X
if __name__=='__main__':
    # small-signal: impulse-like noise at -60 dB
    rng=np.random.default_rng(1)
    N=1<<16
    f=np.fft.rfftfreq(N,1/fs)
    band=(f>=20)&(f<=15000)
    # periodic noise for exact TF
    x0=rng.standard_normal(N)*1e-4
    x=np.tile(x0,3)
    for order in (4,2):
      worst=0;worstph=0
      for L in (27.5,41.2,82.4):
        for gr in (-12,0,12):
          for at in (-12,0,12):
            c,d=modern(x,6,gr,at,L,order)
            X=np.fft.rfft(x[2*N:]);Cc=np.fft.rfft(c[2*N:])/X;D=np.fft.rfft(d[2*N:])/X
            ph=np.angle(D[band]/Cc[band]); 
            sel=np.abs(D[band])>1e-3*np.max(np.abs(D[band]))
            worstph=max(worstph,np.max(np.abs(ph[sel])))
            for m in (.25,.5,.75):
                out=np.abs((1-m)*Cc+m*D)[band]; ideal=((1-m)*np.abs(Cc)+m*np.abs(D))[band]
                worst=max(worst,np.max(np.abs(20*np.log10(out/ideal))))
      print('order',order,'worst crossfade error dB',worst,'worst phase diff deg',np.degrees(worstph))
    c,d=modern(x,6,0,0,41.2,4)
    X=np.fft.rfft(x[2*N:]);Cc=np.fft.rfft(c[2*N:])/X
    for fq in (100,1000,5000,10000,15000,20000):
        i=np.argmin(np.abs(f-fq)); print('clean path mag dB @',fq,round(20*np.log10(abs(Cc[i])),3))
    # group delay of OS identity (low f)
    imp=np.zeros(4096); imp[0]=1
    y=down4(half(half(up4(imp))))
    wg,gd=signal.group_delay((y,[1.0]),w=np.array([50.,1000,5000,10000,15000,20000]),fs=fs)
    print('identity 4x + 2 half-samples: group delay (base samples) at 50/1k/5k/10k/15k/20k',np.round(gd,2))
    # naive: min-phase shelves, no twins (what coherence avoids): one RBJ low shelf -6 dB @120 vs dry
    def rbj_ls(f,g,fs,S=1):
        A=10**(g/40);w0=2*PI*f/fs;al=np.sin(w0)/2*np.sqrt((A+1/A)*(1/S-1)+2);c=np.cos(w0)
        b=[A*((A+1)-(A-1)*c+2*np.sqrt(A)*al),2*A*((A-1)-(A+1)*c),A*((A+1)-(A-1)*c-2*np.sqrt(A)*al)]
        a=[(A+1)+(A-1)*c+2*np.sqrt(A)*al,-2*((A-1)+(A+1)*c),(A+1)+(A-1)*c-2*np.sqrt(A)*al]
        return b,a
    for g in (-6,-12,-18):
        _,H=signal.freqz(*rbj_ls(120,g,fs),worN=np.geomspace(20,2000,300),fs=fs)
        out=np.abs(.5+.5*H); ideal=.5+.5*np.abs(H)
        print('naive shelf %d dB: worst blend loss dB vs ideal'%g,round(np.min(20*np.log10(out/ideal)),2),'max phase',round(np.degrees(np.max(np.abs(np.angle(H)))),1))
    # naive HP (g=0): 2nd order BW HP at 120 vs dry
    b,a=signal.butter(2,120,'hp',fs=fs);_,H=signal.freqz(b,a,worN=np.geomspace(20,2000,300),fs=fs)
    print('naive HP2 at 120: worst loss',round(np.min(20*np.log10(np.abs(.5+.5*H)/(.5+.5*np.abs(H)))),2))
