import os
import json,math
d=json.load(open(os.path.expanduser('~/golem/library/catalog/tunings.json')))
f=lambda m:440*2**((m-69)/12)
def c(a,b): return 1200*math.log2(a/b)
for tid in ['6-std','drop-d','dadgad','7-std','8-std','bass-4-std','bass-5-std']:
  t=next(x for x in d['tunings'] if x['id']==tid); fs=[f(m) for m in t['midi']]
  print(tid)
  for i,fi in enumerate(fs):
    out=[]
    for k in (1,2,3):
      best=(1e9,None)
      for l,fl in enumerate(fs):
        if l==i: continue
        for j in range(1,9):
          dd=abs(c(k*fi,j*fl))
          if dd<best[0]: best=(dd,(t['notes'][l],j))
      out.append(f"k{k}:{best[0]:.1f}c/{abs(k*fi*(2**(best[0]/1200)-1)):.2f}Hz {best[1]}")
    print(' ',t['notes'][i],round(fi,2),' | '.join(out))
