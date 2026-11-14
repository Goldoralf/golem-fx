from t14 import *
print("3 sub-frames, power mean then one TE: non-harmonic dBc +4/-4/+7/-7; f0 98.3,147.1,221.7,311,397")
row=[]
for f0 in (98.3,147.1,221.7,311.0,397.0):
    s,ph,fi=source2(f0,n); x=tract(s,VOW['a'])*0.1
    row.append("/".join(f"{nonharm(run3(x,fi,2**(st/12),3,0,True),f0):.1f}" for st in (4,-4,7,-7)))
print("   ".join(row))
