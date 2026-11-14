import sys; sys.path.insert(0,'.')
from proto import *
from scipy.signal import lfilter
def pb(x, step=0.33, width=1.0, floor=0.0, order=4, nmax=5, purity=True, f_shape=1200):
    fcs=band_centres(step_erb=step); out=np.zeros(len(x))
    for fc in fcs:
        bw=max(width*step*erb(fc), floor)
        z=cbank(x,fc,bw,order); mag=np.abs(z)+1e-12; u=z/mag
        if fc>f_shape: out+=z.real; continue
        acc=np.zeros(len(x)); un=np.ones(len(x),complex)
        for n in range(1,nmax+1):
            un=un*u
            if n%2==1 and n*fc<8000: acc+=un.real/n
        if purity:
            inst=np.angle(u[1:]*np.conj(u[:-1]))*FS/(2*np.pi); inst=np.concatenate([[fc],inst])
            k=np.exp(-1/(0.01*FS)); m=lfilter([1-k],[1,-k],inst); v=lfilter([1-k],[1,-k],(inst-m)**2)
            w=np.exp(-v/(0.25*bw)**2); out+=mag*(w*acc+(1-w)*u.real)
        else: out+=mag*acc
    return out
def lag_ms(order,bw): return order/(np.pi*bw)*1000
sel={k:CHORDS[k] for k in ['E5 power (E2 B2 E3)','open E major','A minor barre V','major third low E2 G#2','major third E4 G#4']}
cfgs=[(4,0.33,0.0),(4,0.33,20.0),(4,0.33,40.0),(3,0.33,20.0),(2,0.33,20.0),(4,0.5,20.0)]
print('lag at 82 Hz / 330 Hz / 1 kHz (ms):')
for o,s,f in cfgs:
    print(f'  o{o} s{s} floor{f}: '+' / '.join('%.0f'%lag_ms(o,max(s*erb(fc),f)) for fc in (82.4,329.6,1000)))
print(f"{'input':28s} "+' '.join(f'o{o} s{s} f{f:.0f}'.rjust(13) for o,s,f in cfgs))
for name,notes in sel.items():
    x=chord(notes)
    print(f"{name:28s} "+' '.join(f'{imd_db(pb(x,s,floor=f,order=o),notes):13.1f}' for o,s,f in cfgs))
