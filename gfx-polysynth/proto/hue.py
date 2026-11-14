import numpy as np
def lab(h):
    c=np.array([int(h[i:i+2],16)/255 for i in (1,3,5)])
    c=np.where(c>0.04045,((c+0.055)/1.055)**2.4,c/12.92)
    M=np.array([[0.4124,0.3576,0.1805],[0.2126,0.7152,0.0722],[0.0193,0.1192,0.9505]])
    xyz=M@c/np.array([0.95047,1,1.08883])
    f=np.where(xyz>0.008856,np.cbrt(xyz),7.787*xyz+16/116)
    return np.array([116*f[1]-16,500*(f[0]-f[1]),200*(f[1]-f[2])])
ex={'dynamics':'#4a5a6e','drive':'#e04a00','utility':'#f0a500','filter':'#00a8c0','eq':'#0058e0','modulation':'#00a84f','delay':'#3a28f0','reverb':'#6a1fd1','pitch':'#c4148c','lofi':'#e8336f','shimmer':'#8a3ff0','gapmeter':'#6b7480','glitch':'#72c000','correct':'#7a1450','acoustic':'#552210','ringmod':'#c9b000','fuzz':'#b0602a','widener':'#c0e0f8','tuner':'#126e1c','radio':'#4b5320','stretch':'#a0a8ff','tape':'#d8c8a0','thrash-drive':'#c8160e'}
for cand in ['#e040ff','#ff4fd8','#b000ff','#00c8a0','#00d0d0','#ff7ab0','#d45cff','#9a00e0','#ff2a9a','#20e0b0','#00e5c0','#e8e000','#ff8a00']:
    L=lab(cand); d=sorted((np.linalg.norm(L-lab(v)),k) for k,v in ex.items())
    print(cand, 'L*%.0f'%L[0], ' '.join('%s %.0f'%(k,dd) for dd,k in d[:3]))
