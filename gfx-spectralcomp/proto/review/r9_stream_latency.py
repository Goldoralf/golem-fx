# Review check 9: Studio as a STREAM (ring buffers, random host block sizes 1..700): is the delay N-1 whatever the blocks? is N-2 possible?
import numpy as np
def stream(x,N,hop_div,D,blocks,gain=None):
    H=N//hop_div; w=0.5-0.5*np.cos(2*np.pi*np.arange(N)/N); norm=1.5*hop_div/4
    inbuf=np.zeros(N); ola=np.zeros(len(x)+2*N); out=np.zeros(len(x)); n=0; pos=0; bad=0
    for B in blocks:
        for _ in range(B):
            if n>=len(x): break
            inbuf=np.roll(inbuf,-1); inbuf[-1]=x[n]
            if (n+1)%H==0:                      # the frame ending at n is processed inside this sample's turn
                X=np.fft.rfft(inbuf*w)
                if gain is not None: X=X*gain
                y=np.fft.irfft(X,N)*w/norm; s=n-N+1
                lo=max(s,0); ola[lo:n+1]+=y[lo-s:]
                done=n-N+1+H                    # samples <= done are complete (the next frame starts at done, with w[0] = 0)
            else: done=-10**9 if n<H-1 else done
            t=n-D
            if t>=0:
                if t>done: bad+=1
                out[n]=ola[t]
            n+=1
    return out,bad
rng=np.random.default_rng(0)
for N in (512,1024):
  for hd in (8,):
    x=rng.standard_normal(6*N)
    for D in (N-1,N-2,N-3):
        res=[]
        for trial in range(3):
            blocks=rng.integers(1,700,size=200) if trial else [64]*1000
            y,bad=stream(x,N,hd,D,blocks); e=y[D+N:]-x[N:len(x)-D]
            res.append((bad,20*np.log10(np.std(e)/np.std(x)+1e-30)))
        print(f"N {N} hop N/{hd} delay {D} (N-{N-D}): samples read before they were complete {[r[0] for r in res]}, null vs delayed input {[round(r[1]) for r in res]} dB")
