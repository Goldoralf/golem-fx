import json,math
d=json.load(open('/home/raph/golem/library/catalog/tunings.json'))
f=lambda m:440*2**((m-69)/12)
c=lambda a,b:1200*math.log2(a/b)
for t in d['tunings']:
    fs=[f(m) for m in t['midi']]; clean=[];shared=[]; tmax=0
    for i,fi in enumerate(fs):
        best=(1e9,None)
        for l,fl in enumerate(fs):
            if l==i: continue
            for j in range(1,9):
                dd=abs(c(fi,j*fl))
                if dd<best[0]: best=(dd,f"{t['notes'][l]}x{j}")
        if best[0]>=150:
            hz=fi*(2**((best[0]-60)/1200)-1); T=min(1.0,max(0.25,4/hz)); tmax=max(tmax,T)
            clean.append(f"{t['notes'][i]}({T:.2f}s)")
        else: shared.append(f"{t['notes'][i]}~{best[1]}({best[0]:.0f}c)")
    print(f"{t['id']:14s} clean: {' '.join(clean):60s} | shared: {' '.join(shared)}  | first full read {tmax:.2f}s")
