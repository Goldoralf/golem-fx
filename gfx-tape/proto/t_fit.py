import numpy as np
from scipy.optimize import least_squares
f = np.geomspace(50, 20000, 300)
def shelf(f, fc, g):  # first-order high shelf, gain g dB (cut), corner fc (midpoint)
    A = 10**(g/20); s = 1j*f/fc; return (1 + s*np.sqrt(A))/(1 + s/np.sqrt(A)) if False else (np.sqrt(A)*s+1)/(s/np.sqrt(A)+1)
print("A. README: 3 fixed first-order shelves (1.5, 5, 14 kHz) + broadband gain; max |err| dB where target >= -30 dB")
for L10 in (1, 3, 6, 12, 23, 40, 57):   # loss at 10 kHz in dB
    tgt = -L10*f/1e4; m = tgt >= -30
    def r(x): return (20*np.log10(np.abs(shelf(f,1500,x[0])*shelf(f,5000,x[1])*shelf(f,14000,x[2])))+x[3]-tgt)[m]
    s = least_squares(r, [-1,-1,-1,0]); print(f"  loss@10k {L10:3d} dB: max err {np.max(abs(s.fun)):.2f} dB, gains {np.round(s.x,1)}")
print("B. N one-pole low-passes, corners scaled with v/d (one fit in x = f/f0, target -8.686 x dB i.e. e^-x), fit to -30 dB")
x = np.geomspace(0.003, 30/8.686, 400); tgt = -8.686*x
for N in (3, 4, 5, 6):
    def r(c): return sum(-10*np.log10(1+(x/ci)**2) for ci in c) - tgt
    s = least_squares(r, np.linspace(1, 3, N)); print(f"  N={N}: max err {np.max(abs(s.fun)):.2f} dB, corners/f0 {np.round(np.sort(np.abs(s.x)),3)}")
