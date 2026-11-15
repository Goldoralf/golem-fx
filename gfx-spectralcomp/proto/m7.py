import numpy as np, sc
rms=lambda v: 20*np.log10(np.sqrt(np.mean(v**2)))
print("== sample rates: 1 kHz and 6 kHz sines at -12 dBFS, down 4 (formula -18.0 dB); Studio window 2048, Live 1024")
for fs in (44100,48000,96000,192000):
    row=[]
    for mode,W in (('studio',2048),('live',1024)):
        for f0 in (1000,6000):
            n=int(0.9*fs); x=10**(-12/20)*np.sin(2*np.pi*f0*np.arange(n)/fs)
            y,A=sc.run(x,fs,mode=mode,window=W,attack=5,release=5,knee=0,auto_gain=False,return_all=True)
            D=A['D']; s=slice(int(0.6*fs),int(0.85*fs)); g=rms(y[s])-rms(x[s.start-D:s.stop-D])
            row.append(f"{mode} {f0}: {g:.2f}")
    print(fs, "N(2048)=%d"%sc.n_for(2048,fs), '; '.join(row))
fs=48000
print("== Live mix 50 %: pink noise, a fixed -12 dB third-octave cut at 1 kHz; deepest dip of |0.5 + 0.5 H| vs the ideal blend")
eq=sc.LiveEQ(fs); G=eq.geq
for depth in (-6,-12,-24,+9):
    t=np.where(np.isclose(G.fp,1000),depth,0.0)
    gs,n=G.solve(None,iters=8,T=lambda f: np.interp(np.log2(f),np.log2(G.fp),t))
    f=np.geomspace(100,10000,2000); w=2*np.pi*f/fs; z=np.exp(1j*w); H=np.ones_like(z)
    for B in G.designs(gs):
        b0,b1,b2,a1,a2=B; H*=(b0+b1/z+b2/z**2)/(1+a1/z+a2/z**2)
    mixc=np.abs(0.5+0.5*H); ideal=0.5+0.5*np.abs(H)
    print(f"cut {depth:+d} dB: worst (complex blend - magnitude blend) {20*np.log10(np.min(mixc/ideal)):.2f} dB; max phase of H {np.degrees(np.max(np.abs(np.angle(H)))):.0f} deg")
print("== Key mode: main = steady pink noise -24 dBFS; key = E4 pluck train; down 8, thresh -50, attack 10, release 150")
rng=np.random.default_rng(2); n=4*fs
X=np.fft.rfft(rng.standard_normal(n)); f=np.fft.rfftfreq(n,1/fs); X[1:]/=np.sqrt(f[1:]); X[0]=0; main=np.fft.irfft(X,n); main*=10**(-24/20)/np.std(main)
key=np.zeros(n)
for k in range(4):
    s=sc.pluck(329.63,0.8,fs,seed=k); key[int((0.5+0.9*k)*fs):int((0.5+0.9*k)*fs)+len(s)]+=s*0.3
for mode,W in (('studio',2048),('live',1024)):
    y,A=sc.run(main,fs,key=key,mode=mode,window=W,down=8,thresh=-50,attack=10,release=150,auto_gain=False,return_all=True); D=A['D']
    Y=np.abs(np.fft.rfft(y[D+int(0.55*fs):D+int(0.75*fs)]))**2; Xm=np.abs(np.fft.rfft(main[int(0.55*fs):int(0.75*fs)]))**2; ff=np.fft.rfftfreq(int(0.2*fs),1/fs)
    def band(fc): m=(ff>fc/2**(1/6))&(ff<fc*2**(1/6)); return 10*np.log10(Y[m].sum()/Xm[m].sum())
    print(mode, "gain during a note at 330 / 660 / 990 Hz: %.1f / %.1f / %.1f dB; at 200 / 3000 / 8000 Hz: %.1f / %.1f / %.1f dB"%tuple(band(fc) for fc in (330,660,990,200,3000,8000)))
