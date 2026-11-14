import numpy as np
from pickup import inv,PU,db,aw
f=np.array([4000,6000,8000,10000,16000.]); ff=np.linspace(20,20000,4000)
for k,fp in (('SC',12e3),('HB',8.7e3),('P90',7.2e3),('HB',12e3),('P90',12e3)):
    HB=inv(f,*PU[k],fp=fp)[1]; W=inv(ff,*PU[k],fp=fp)[1]
    print(k,fp,np.round(db(HB),1),'max',round(db(W).max(),1),'A-w noise',round(10*np.log10(np.sum(abs(W)**2*aw(ff)**2)/np.sum(aw(ff)**2)),1))
Ls=.648; gm=166.409; c=2*Ls*gm
for nm,x in [('hole200',.2),('hole165',.165),('neck',.16),('mid',.1),('bridge',.041)]: print(nm,round(c/(2*np.pi*np.sqrt(2)*x)))
def R(fq,xe,xs): s=2j*np.pi*fq; return abs((s+c/xe/np.sqrt(2))/(s+c/xs/np.sqrt(2)))
for nm,x in [('neck',.16),('mid',.1),('bridge',.041)]:
    print(nm,[round(20*np.log10(R(q,x,.2)),1) for q in (50,120,300,600,1500,3000)])
print('B-N@50',round(20*np.log10(R(50,.041,.2)/R(50,.16,.2)),2),'@3k',round(20*np.log10(R(3000,.041,.2)/R(3000,.16,.2)),2))
