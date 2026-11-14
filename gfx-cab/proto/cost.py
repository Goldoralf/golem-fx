import numpy as np
C=64
SPEC=[(64,64,7),(256,512,6),(1024,2048,6),(4096,8192,6),(8192,32768,None)]
STAG=[0,0,1,2,3]
fft=lambda B:2.5*2*B*np.log2(2*B)
def profile(L, class_len, nk=1, split=1, live=True):
    period=8192//C; w=np.zeros(period)+C*C*2*nk
    for li,(B,s,n) in enumerate(SPEC):
        nl=max(0,-(-(class_len-s)//B)); nl=min(nl,n) if n else nl
        nu=min(nl,max(0,-(-(L-s)//B)))
        if nl==0 or (not live and nu==0): continue
        ns=B//C
        for c in range(period):
            slot=c%ns-STAG[li]
            if s==B:
                w[c]+=fft(B)+ (nk*(8*(B+1)*nu+fft(B)) if nu else 0); continue
            last=ns-1-STAG[li]
            if slot<0: continue
            sp=min(split,last//2) if B>=1024 else 1
            # fwd fft in slots 0..sp-1 ; ifft in slots last-sp+1..last ; MACs spread over slots sp-1 .. last-sp
            if slot<sp: w[c]+=fft(B)/sp
            if nu:
                if slot>last-sp: w[c]+=nk*fft(B)/sp
                m0,m1=sp-1 if sp>1 else 0,(last-sp if sp>1 else last-1)
                m1=max(m1,m0)
                if m0<=slot<=m1: w[c]+=nk*8*(B+1)*nu/(m1-m0+1)
    return w.mean()/C, w.max()/w.mean(), w.max()
print("flop/sample, worst chunk/mean, worst chunk flops (Mflop)")
for name,sr,sec,cl in [("cab 200ms",48000,.2,2),("cab 200ms",96000,.2,2),("cab 200ms",192000,.2,2),("cab 500ms",48000,.5,2),("cab 2s",48000,2,2),("cab 2s",192000,2,2),("spring 4s",48000,4,20),("irfx 20s",48000,20,20),("irfx 20s",192000,20,20)]:
    L=int(sr*sec); CL=int(sr*cl)
    for split in (1,4,8):
        a=profile(L,CL,split=split); 
        print(f"{name:10s} {sr:6d} split{split}: {a[0]:7.0f}  x{a[1]:5.2f}  {a[2]/1e6:5.2f}", end=" | ")
    b=profile(L,CL,nk=2); d=profile(L,CL,live=False)
    print(f" 2 kernels {b[0]:6.0f}; not-live {d[0]:6.0f}")
