import sys; sys.path.insert(0,'.')
from proto import *

def covered(notes):
    """notes whose fundamental is partial h<=8 of a lower chord note within 15 cents"""
    out=set()
    for a in notes:
        for b in notes:
            if b<a:
                d=a-b
                for h in range(2,9):
                    if abs(12*np.log2(h)-d)*100<15: out.add(a)
    return out

def name2(spec, mids, lo=40, hi=76, maxn=6, floor=0.2, fund_min=0.15):
    r=spec.astype(float).copy(); i0=mids[0]; found=[]; first=None
    excluded=set()
    for _ in range(maxn+6):
        best,bm,bA=0,None,None
        for m in range(lo,hi+1):
            if m in found or m in excluded: continue
            A=[]
            for off in H_OFF:
                j=m+off-i0
                if j>=len(r): break
                A.append(r[j])
            A=np.array(A); h=np.arange(1,len(A)+1)
            s=np.sum(A*(0.5+1.0/h))
            if s>best: best,bm,bA=s,m,A
        if bm is None: break
        if first is None: first=best
        elif best<floor*first: break
        if r[bm-i0] < fund_min*spec.max():
            excluded.add(bm); continue
        found.append(bm)
        if len(found)>=maxn: break
        # smooth partial estimate (Klapuri): min(A_h, moving average of the partial amplitudes)
        Ap=np.pad(bA,1,mode='edge'); sm=(Ap[:-2]+Ap[1:-1]+Ap[2:])/3
        est=np.minimum(bA, sm*1.0)
        for hh,off in enumerate(H_OFF[:len(bA)]):
            j=bm+off-i0
            r[j]=max(0.0,r[j]-est[hh])
    return sorted(found)

def run(rel_bw,min_bw,order=2):
    print(f'-- bands {rel_bw} st wide, floor {min_bw} Hz, order {order}')
    for name,notes in CHORDS.items():
        x=chord(notes,dur=0.7)
        mids,E=cq_envelopes(x,rel_bw=rel_bw,min_bw=min_bw,order=order)
        truth=sorted(set(notes)-covered(notes))
        t_last=0.07; ok=None; t_ok=None; last=None
        for i in range(int(t_last*FS),int(0.65*FS),48):
            g=name2(E[:,i],mids)
            if g==truth:
                if ok is None: ok=i
                elif i-ok>=int(0.05*FS) and t_ok is None: t_ok=(ok/FS-t_last)*1000
            else:
                ok=None
            last=g
        print(f"   {name:26s} truth {truth} named {('%.0f ms'%t_ok) if t_ok is not None else 'never':>8s}  final {last}")
for a in [(0.5,6.0),(1.0,10.0),(1.0,20.0)]:
    run(*a)
