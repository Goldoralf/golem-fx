import numpy as np
np.set_printoptions(precision=2, suppress=True, linewidth=200)
def bell_db(f, f0, q, g):
    a = 10**(g/40); w = f/f0
    num = (1-w*w)**2 + (a/q*w)**2; den = (1-w*w)**2 + (w/(a*q))**2
    return 10*np.log10(num/den)
def bp_db(f, f0, q, order=1):
    w=f/f0; return -10*order*np.log10(1+q*q*(w-1/w)**2)
def Qoct(N): return np.sqrt(2**N)/(2**N-1)
f = np.geomspace(30, 20000, 4000)
def bank(nb, f_lo=80., f_hi=None, per_oct=2):
    fk = f_lo*2**(np.arange(nb)/per_oct); return fk
for nb, po, qs in [(16,2,[2.87, 2.0, 1.41]), (24,3,[4.32,2.87]), (32,4,[5.76,4.32])]:
    fk = bank(nb, per_oct=po)
    inband = (f>=fk[1])&(f<=fk[-2])
    for q in qs:
        for G in [6, 12, 18]:
            tot = sum(bell_db(f, f0, q, -G) for f0 in fk)
            c = np.array([ sum(bell_db(fj, f0, q, -G) for f0 in fk) for fj in fk])
            print(f"nb {nb} top {fk[-1]:.0f} Q {q:.2f} uniform -{G}: cut at centres {(-c[1:-1]).min():.1f}..{(-c[1:-1]).max():.1f} dB, in-band ripple {np.ptp(tot[inband]):.2f} dB, mean {-tot[inband].mean():.1f}")
# interaction matrix at 16 bands, Q 2.87, G=12
fk = bank(16); q=2.87
for G in [3, 12, 24]:
    B = np.array([[bell_db(fj, f0, q, -G)/-G for f0 in fk] for fj in fk])
    print("G",G,"row of band 8:", B[8, 4:13], "row sum", B[8].sum())
# single band -12: response 1 and 2 octaves away
for q in [2.87, 2.0]:
  for G in [12, 18, 30]:
    print(f"single bell Q{q} -{G} dB at 1 kHz: at 1.41k {bell_db(1414.,1000.,q,-G):.2f}, 2k {bell_db(2000.,1000.,q,-G):.2f}, 4k {bell_db(4000.,1000.,q,-G):.2f}")
# analysis leakage: 1 kHz sine read by BP at each band
for order in [1,2]:
  for q in [2.87, 4.0]:
    K = np.array([bp_db(1000., f0, q, order) for f0 in fk])
    print(f"BP order {order*2} Q{q}: level of 1k sine in bands (dB re max):", (K-K.max())[3:13])
