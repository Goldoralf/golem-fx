import numpy as np, scipy.signal as sg
fs=48000; f0=1200.0*1.00731  # avoid commensurate
def clipF(x): # antiderivative of hard clip at +-1
    return np.where(np.abs(x)<=1,0.5*x*x,np.abs(x)-0.5)
def adaa(x):
    d=np.diff(x,prepend=x[0]-1e-3); xm=0.5*(x+np.concatenate(([x[0]],x[:-1])))
    y=np.where(np.abs(d)>1e-9,(clipF(x)-clipF(np.concatenate(([x[0]],x[:-1]))))/np.where(np.abs(d)>1e-9,d,1),np.clip(xm,-1,1))
    return y
def measure(y,lab,fmax):
    y=y[len(y)//4:]; n=len(y); w=sg.windows.blackmanharris(n); Y=np.abs(np.fft.rfft(y*w))/w.sum()*2; f=np.fft.rfftfreq(n,1/fs)
    i0=int(round(f0*n/fs)); a1=Y[i0-5:i0+6].max()
    m=(f<fmax)&(f>20)
    for k in range(1,int(fs/2/f0)+2): m&=np.abs(f-k*f0)>30
    return 20*np.log10(Y[m].max()/a1)
for gdb in (30,):
  for os in (1,2,4):
    n=fs*os*4; t=np.arange(n)/(fs*os); x=10**(gdb/20)*np.sin(2*np.pi*f0*t)
    for use in (False,True):
        y=adaa(x) if use else np.clip(x,-1,1)
        if os>1: y=sg.resample_poly(y,1,os,window=('kaiser',16))
        # speaker body: megaphone 4.5 kHz 4th-order LP ; laptop-like 14 kHz 2nd
        yb=sg.sosfilt(sg.butter(4,4500,fs=fs,output='sos'),y)
        yc=sg.sosfilt(sg.butter(4,6000,fs=fs,output='sos'),y)
        print(f'gain +{gdb} dB os {os}x adaa {use!s:5}: worst alias full band {measure(y,"",20000):6.1f} dB | after 4.5k LP4 {measure(yb,"",20000):6.1f} | after 6k LP4 {measure(yc,"",20000):6.1f}  (re fundamental)')
# crossover distortion x - d tanh(x/d), d=0.02, low battery d=0.1
for d in (0.01,0.02,0.1):
  for os in (1,2):
    n=fs*os*4; t=np.arange(n)/(fs*os); x=np.sin(2*np.pi*f0*t)
    y=x-d*np.tanh(x/d)
    if os>1: y=sg.resample_poly(y,1,os,window=('kaiser',16))
    print(f'crossover d={d} os {os}x: worst alias {measure(y,"",20000):6.1f} dB')
# carbon guard tanh then rational at 2x: input 1.2k after LP, +30 dB and +12 dB into 0.6*tanh(x/0.6)
for gdb in (12,30):
  for os in (2,4):
    n=fs*os*4; t=np.arange(n)/(fs*os); a=0.1*10**(gdb/20); x=a*np.sin(2*np.pi*f0*t)
    hp=0.6*np.tanh(x/0.6); y=1/(1-hp)-1
    y=sg.resample_poly(y,1,os,window=('kaiser',16))
    print(f'carbon drive +{gdb} dB (h.p peak {a:.2f}) os {os}x: worst alias {measure(y,"",20000):6.1f} dB')
