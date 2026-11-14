import numpy as np
exec(open('design.py').read().split("import sys")[0])
def gd(co,fs,f,d=0.01):
    A1,_=resp(co,fs,np.array([f-d])); A2,_=resp(co,fs,np.array([f+d]))
    return -(np.angle(A2/A1))[0]/(2*np.pi*2*d)*1000
for fs in (48000,96000):
  for lo in (15,30,40,60):
    for n in (12,16,20):
        c,q=exact(n,2*lo/fs)
        print(fs,'edge',lo,n//2,'+',n//2,'rej %.1f dB'%(6.02+5*(2*n+1)*np.log10(q)),'gd ms @41 %.2f @82 %.2f @200 %.2f @1k %.2f'%tuple(gd(c,fs,f) for f in (41,82,200,1000)))
