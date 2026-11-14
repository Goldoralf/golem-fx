from geq import *
for sr in (48000,):
    for nm,c in (('OCT10',OCT10),('OCT11',OCT),('GT7',GT),('GB7',GB)):
        run(f"{nm} {sr} README(half,c.3)",GEQ(c,sr,edge='half'))
        run(f"{nm} {sr} ACGE(neigh,c.3)",GEQ(c,sr,edge='neigh'))
