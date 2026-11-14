from chain import *
N=1<<15
def bins(y,k): return np.fft.rfft(y[-N:])[k]
def merged(x,drive,gr,L):
    d=x.copy(); c=x.copy()
    for f,k,g in [(4*L,'lo',-6+gr),(1500,'hi',3)]:
        d=section(d,f,fs,db(g) if k=='lo' else 1,db(g) if k=='hi' else 1,2); c=twin(c,f,fs,2)
    d=up4(d); c=up4(c); d=adaa(d*db(drive),.25); c=half(c)
    d=section(d,3000,4*fs,1,db(-6),2); c=twin(c,3000,4*fs,2)
    d=adaa(d*db(12),0.0); c=half(c); d=down4(d); c=down4(c)
    d=section(d,6500,fs,1,0,4); c=twin(c,6500,fs,4); return c,d
def bare(x,drive,gr,L):
    return x, np.tanh(db(12)*(np.tanh(x*db(drive)+.25)-np.tanh(.25)))
rng=np.random.default_rng(3)
for name,fn in (('draft LR4 x4',lambda x,dr,gr,L:modern(x,dr,gr,0,L,4)),('merged LR2',merged),('bare clipper, no filters',bare)):
  for prof,amps in (('weak fund (0.5,1,.6..)',[0.5,1.0,0.6,0.35,0.25,0.15,0.1,0.07]),('strong fund (1,.5,.33..)',[1,.5,.33,.25,.2,.15,.1,.07])):
    loss=[];ph=[]
    for t in range(60):
        f0=rng.choice([30.87,41.2,55,82.4]); k0=int(round(f0*N/fs)); n=np.arange(3*N)
        phs=rng.uniform(0,2*PI,8)
        x=sum(a*np.sin(2*PI*k0*(i+1)*n/N+p) for i,(a,p) in enumerate(zip(amps,phs))); x*=db(-12)/np.max(np.abs(x))
        dr=rng.choice([6,20,34,48]); gr=rng.choice([-12,-6,0,6])
        c,d=fn(x,dr,gr,41.2)
        k=np.sqrt(np.mean(c[-N:]**2)/np.mean(d[-N:]**2))
        C=bins(c,k0);D=k*bins(d,k0)
        loss.append(20*np.log10(abs(C+D)/(abs(C)+abs(D)))); ph.append(abs(np.degrees(np.angle(D/C))))
    loss=np.array(loss);ph=np.array(ph)
    print(f'{name:26s} {prof:26s} fundamental: |phase| median {np.median(ph):5.1f} p90 {np.percentile(ph,90):5.1f} max {ph.max():5.1f} deg; blend-50 loss median {np.median(loss):.2f} p90 {np.percentile(loss,10):.2f} worst {loss.min():.2f} dB')
