from det import *
x=np.sin(2*np.pi*41.2*tt); x=x/np.sqrt(np.mean(x**2))*10**(-12/20)
for att,rel in ((1,250),(1,75),(1,20),(10,20),(30,250)):
    for char in ('clean','bus'):
        out=[]
        for mode in ('today','draft','review'):
            g=run(x,mode,30.0,-20.0,4.0,att_ms=att,rel_ms=rel,char=char); y=x*10**(g/20)
            _,h=thd(y,41.2,3*fs); out.append('%s: ripple %.3f dB H3 %.0f dBc'%(mode,g[3*fs:].max()-g[3*fs:].min(),h[1]))
        print('att %g rel %g %-5s | '%(att,rel,char)+' | '.join(out))
# mid band: f_low = 120 Hz -> 10 ms window; a 100 Hz note leaking in (asymmetric)
f0=100.; xa=np.sin(2*np.pi*f0*tt)+0.6*np.sin(2*np.pi*2*f0*tt+0.9); xa=xa/np.sqrt(np.mean(xa**2))*10**(-12/20)
for mode in ('today','draft','review'):
    g=run(xa,mode,120.0,-20.0,4.0,att_ms=1,rel_ms=20); print('lm band, 100 Hz asym note, flow 120:',mode,'ripple %.3f dB'%(g[3*fs:].max()-g[3*fs:].min()))
