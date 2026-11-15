import numpy as np, sc, time
from scipy.io import wavfile
fs=48000
def rd(f): sr,x=wavfile.read('/home/raph/golem/library/di-refs/guitar/'+f); return (x/2**31)[:6*fs]
sigs={'clean DI':rd('clean.wav'),'lead-clean DI':rd('lead-clean.wav'),'synthetic phrase':sc.guitar_phrase(fs)}
rms=lambda v: 20*np.log10(np.sqrt(np.mean(v**2)))
print("== level vs bypass, defaults (thresh -36, down 4, up 1, attack 150, release 300, 1/6 oct), auto gain on / off")
for nm,x in sigs.items():
    row=[]
    for mode,W in (('live',1024),('studio',2048)):
        for ag in (True,False):
            y,A=sc.run(x,fs,mode=mode,window=W,auto_gain=ag,return_all=True); D=A['D']
            row.append(f"{mode}{W} ag{int(ag)} {rms(y[D:])-rms(x[:len(x)-D]):+.1f}")
    print(f"{nm:16s} peak {20*np.log10(np.abs(x).max()):.1f} dBFS, RMS {rms(x):.1f}: "+'; '.join(row), "(makeup %.1f dB)"%A['makeup'])
print("== an 'even' setting: thresh -40, down 3, up 2, attack 30, release 200 (level and the octave-band crest)")
for nm,x in sigs.items():
    for mode,W in (('live',1024),('studio',1024)):
        t0=time.time(); y,A=sc.run(x,fs,mode=mode,window=W,thresh=-40,down=3,up=2,attack=30,release=200,return_all=True); D=A['D']
        e=A.get('err',[]); 
        print(f"{nm:16s} {mode}: level {rms(y[D:])-rms(x[:len(x)-D]):+.1f} dB, makeup {A['makeup']:.1f}"+(f", ACGE realised-vs-target worst {np.max(e):.2f} dB, p95 {np.percentile(e,95):.2f}" if e else "")+f"  ({time.time()-t0:.0f} s)")
