import numpy as np, sc
exec(open('r2_static_spurs.py').read().split("run(\"design as written (band")[0])
run("hop N/16",orig,hop_div=16)
def be3(N,fs,res_st,hmin=2.0): return orig(N,fs,res_st,3.0)
run("band >= +-3 bins",be3)
