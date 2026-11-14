import sys; sys.path.insert(0,'.')
from proto import *
from p2 import covered

def sal(r,m,i0):
    A=[]
    for off in H_OFF:
        j=m+off-i0
        if j>=len(r) or j<0: break
        A.append(r[j])
    A=np.array(A); h=np.arange(1,len(A)+1)
    return np.sum(A*(0.5+1.0/h)),A

def name3(spec, reg, mids, lo=40, hi=76, maxn=6, floor=0.2, fund_min=0.12):
    r=spec.astype(float).copy(); i0=mids[0]; found=[]; first=None; excl=set()
    for _ in range(maxn+8):
        S={m:sal(r,m,i0) for m in range(lo-1,hi+2)}
        best,bm=0,None
        for m in range(lo,hi+1):
            if m in found or m in excl: continue
            s=S[m][0]
            if s>best and s>=S[m-1][0] and s>=S[m+1][0]: best,bm=s,m
        if bm is None: break
        if first is None: first=best
        elif best<floor*first: break
        if reg[bm-i0] < fund_min*reg.max():
            excl.add(bm); continue
        found.append(bm)
        if len(found)>=maxn: break
        bA=S[bm][1]
        Ap=np.pad(bA,1,mode='edge'); sm=(Ap[:-2]+Ap[1:-1]+Ap[2:])/3
        est=np.minimum(bA,sm)
        for hh,off in enumerate(H_OFF[:len(bA)]):
            j=bm+off-i0; r[j]=max(0.0,r[j]-est[hh])
    return sorted(found)

def run(rel_bw,min_bw,reg_bw,spread=0.02,label=''):
    print(f'-- salience bands {rel_bw} st, floor {min_bw} Hz; register bands {reg_bw} Hz; strum spread {spread*1000:.0f} ms {label}')
    res=[]
    for name,notes in CHORDS.items():
        x=chord(notes,dur=0.7,spread=spread)
        mids,E=cq_envelopes(x,rel_bw=rel_bw,min_bw=min_bw)
        _,R=cq_envelopes(x,rel_bw=10,min_bw=reg_bw)
        must=set(notes)-covered(notes); allowed=set(notes)
        t_last=0.05+spread; ok=None; t_ok=None; errs=0; frames=0
        for i in range(int(t_last*FS),int(0.65*FS),48):
            g=set(name3(E[:,i],R[:,i],mids))
            good = must<=g<=allowed
            if good:
                if ok is None: ok=i
                elif i-ok>=int(0.05*FS) and t_ok is None: t_ok=(ok/FS-t_last)*1000
            else:
                ok=None
            if t_ok is not None and not good: errs+=1
            if t_ok is not None: frames+=1
        print(f"   {name:26s} must {sorted(must)} named {('%.0f ms'%t_ok) if t_ok is not None else 'never':>8s}  wrong frames after {errs}/{frames}")
for a in [(0.5,6.0,25.0),(0.5,4.0,30.0),(0.7,6.0,30.0)]:
    run(*a)
run(0.5,6.0,25.0,spread=0.0)
