import numpy as np, re, sys
src=open('vib3.py').read()
core=src.split("def ss(")[0]
for win,shape in [(1.25,0.3),(1.0,0.3),(1.0,0.4),(0.75,0.3)]:
    ns={}
    exec(core.replace("1.25*fs/f",f"{win}*fs/f"),ns)
    run=ns['run']; fs=200.0
    def ss(t,t0,d):
        u=np.clip((t-t0)/d,0,1); return u*u*(3-2*u)
    t=np.arange(int(3*fs))/fs; fl=lambda tg:int(np.sum(np.diff(tg)!=0)); rng=np.random.default_rng(1); J=4.0
    eng=[];reeng=[];late=[];flips=[]
    for depth,fv,step,dur in [(30,6,200,.06),(50,6,200,.06),(50,5,200,.06),(50,7,200,.06),(100,6,200,.06),(50,6,100,.06),(100,5.5,100,.08),(50,6,200,.2),(50,6,-300,.05),(20,6.5,100,.05),(70,4.5,-100,.1)]:
        p=20+depth*np.sin(2*np.pi*fv*(t-0.3))*ss(t,0.3,0.2)+step*ss(t,1.5,dur)+rng.normal(0,J,len(t))
        o=run(p,shape=shape); tg=o['t']; goal=100*round((20+step)/100)
        idx=np.where((t>1.4)&(tg==goal))[0]; late.append((t[idx[0]]-1.5-dur/2)*1000)
        e=np.where(o['w']>0.99)[0]; eng.append(1000*(t[e[0]]-0.3) if len(e) else 9999)
        r=np.where((o['w']>0.99)&(t>1.5+dur))[0]; reeng.append(1000*(t[r[0]]-1.5-dur) if len(r) else 9999); flips.append(fl(tg))
    fp=[]
    p=np.zeros_like(t)
    for k in range(1,12): p+=(200 if k%2 else -200)*ss(t,0.5+0.15*k,0.04)
    fp.append(run(p+rng.normal(0,J,len(t)),shape=shape)['w'].max())
    p=np.zeros_like(t)
    for k in range(1,12): p+=(100 if k%2 else -100)*ss(t,0.5+0.15*k,0.04)
    o=run(p+rng.normal(0,J,len(t)),shape=shape); fp.append(o['w'].max()); mf=fl(o['t'])
    p=50*np.tanh(3*np.sin(2*np.pi*6*t))+50+rng.normal(0,J,len(t)); fp.append(run(p,shape=shape)['w'].max())
    p=-80*(1-ss(t,0.05,0.12))+rng.normal(0,J,len(t)); fp.append(run(p,shape=shape)['w'].max())
    p=30*np.sin(2*np.pi*0.7*t)+rng.normal(0,8,len(t)); fp.append(run(p,shape=shape)['w'].max())
    p=60*ss(t,1.0,0.5)+rng.normal(0,J,len(t)); fp.append(run(p,shape=shape)['w'].max())
    print(f"win {win} shape {shape}: engage {np.round(eng)} | re-engage med {np.median(reeng):.0f} | step late {np.round(late)} | flips {flips} | false w_v max (mel200, mel100, trill, scoop, drift, bend) {np.round(fp,2)} mel100 flips {mf}")
