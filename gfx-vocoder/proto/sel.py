import numpy as np
fs=48000.
def erb(f): return 24.7*(4.37*f/1000+1)
def erbnum(f): return 21.4*np.log10(1+0.00437*f)
def inv(e): return (10**(e/21.4)-1)/0.00437
def H(f,fc,bw,order):
    a=np.exp(-2*np.pi*bw/fs+2j*np.pi*fc/fs); z=np.exp(2j*np.pi*f/fs)
    return abs(((1-abs(a))/(1-a/z))**order)
for n in (16,24,40):
  e=np.linspace(erbnum(100),erbnum(7000),n);fc=inv(e);sp=e[1]-e[0];k=n//2
  for order,w in ((2,1.4),(4,0.85)):
    b=lambda i: w*erb(fc[i])*sp
    att=[20*np.log10(H(fc[k],fc[k+j],b(k+j),order)/H(fc[k+j],fc[k+j],b(k+j),order)) for j in (1,2,3)]
    print(n,order,f"fc={fc[k]:.0f} spacing {sp:.2f} ERB, neighbour response at fc_k: "+", ".join(f"{a:.1f}" for a in att))
