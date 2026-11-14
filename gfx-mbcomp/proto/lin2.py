from lin import *
# rule: LR8's Q=1.307 all-pass section of a crossover below FMIN stays minimum-phase (not in the kernel)
def rule(xs,order,L,fmin,B=64):
    K=L-B; inc=[]; exc=[]
    for fc in xs:
        aps=split(fc,order)[2]
        if order==8 and fc<fmin: inc.append(aps[0]); exc.append(aps[1])
        else: inc+=aps
    g=kern(inc,K); G=signal.freqz(g,1,worN=f,fs=fs)[1]
    H=G*fr(inc,f); E=H-np.exp(-2j*np.pi*f/fs*K)
    mag=20*np.log10(np.abs(G))
    # remaining dispersion: group delay of excluded sections
    gd=0
    if exc:
        A=fr(exc,f); gd=(-np.gradient(np.unwrap(np.angle(A)),2*np.pi*f)*1e3)
        gdtxt='left: gd %.1f ms @20Hz, peak %.1f ms @%.0f Hz'%(gd[0],gd.max(),f[gd.argmax()])
    else: gdtxt='all corrected'
    print('LR%d %-16s fmin %d: resid(vs target) %6.1f dB mag +%.3f/%.3f | %s'%(order,xs,fmin,20*np.log10(np.abs(E).max()),mag.max(),mag.min(),gdtxt))
for xs in ([40,2500],[40,150,800],[60,2500],[70,2500],[79,600,2500],[80,600,2500],[90,2500],[100,1500]):
    rule(xs,8,2048,80)
for xs in ([40,2500],[40,150,800],[50,2500]):
    rule(xs,4,2048,80)
# minimum mode dispersion for the reachable worst case
ff=np.geomspace(10,23000,6000)
for order in (4,8):
    for xs in ([40,800],[40,150,800]):
        b,A=tree(xs,order,ff); gd=-np.gradient(np.unwrap(np.angle(A)),2*np.pi*ff)*1e3
        print('min-phase LR%d'%order,xs,'gd 20Hz %.1f peak %.1f@%.0f 1k %.2f'%(gd[np.searchsorted(ff,20)],gd.max(),ff[gd.argmax()],gd[np.searchsorted(ff,1000)]))
