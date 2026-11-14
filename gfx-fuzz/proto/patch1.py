s = open('muff.py').read()
s = s.replace("def muff(fs,start=1,active=None,sustain=1.0):", "def muff(fs,start=1,active=None,sustain=1.0,vdc=0.0):")
s = s.replace("c.v(drive,'0',0.)", "c.v(drive,'0',vdc)")
s = s.replace("""        c=muff(fs,start=k,active={k},sustain=sus)
        dcv=c.volt(c.x_op,{1:'in',2:'sw',3:'c2',4:'c3'}[k])""", """        c=muff(fs,start=k,active={k},sustain=sus,vdc=0.0 if k==1 else cj.volt(cj.x_op,{2:'sw',3:'c2',4:'c3'}[k]))""")
s = s.replace("c=muff(fs,sustain=sus); yA,its,bad=c.run(", "cj=c=muff(fs,sustain=sus); yA,its,bad=c.run(")
s = s.replace("maxit=50)", "maxit=20,cont=True)")
assert 'vdc=0.0 if' in s
open('muff.py', 'w').write(s)
s = open('sim.py').read()
s = s.replace("""            vn = self.PJ @ x
            d = np.abs(vn - v)[~self.fz]""", """            if not np.isfinite(x).all():
                return x, v0, it, False
            vn = self.PJ @ x
            d = np.abs(vn - v)[~self.fz]""")
s = s.replace("""            x, v, its[n], ok = self.newton(b, seed, tol, maxit, lim)
            bad[n] = not ok""", """            xs, vs = x, v
            x, v, its[n], ok = self.newton(b, seed, tol, maxit, lim)
            if not ok and cont:
                for K in (4, 16):
                    vv = vs
                    for k in range(1, K + 1):
                        x, vv, i2, ok = self.newton(bprev + (b - bprev) * k / K, vv, tol, maxit, lim)
                        its[n] += i2
                        if not ok: break
                    if ok: v = vv; break
                if not ok: x, v = xs, vs
                self.nfall += 1
            bprev = b
            bad[n] = not ok""")
s = s.replace("        b0 = self.rhs_src()\n        for n in range(len(u)):", "        b0 = self.rhs_src(); bprev = b0.copy(); self.nfall = 0\n        for n in range(len(u)):")
s = s.replace("lim='pnjlim', extrap=True, x0=None):", "lim='pnjlim', extrap=True, x0=None, cont=False):")
assert 'nfall += 1' in s
open('sim.py', 'w').write(s)
s = open('alias.py').read()
s = s.replace("maxit=50)", "maxit=20,cont=True)").replace("its.mean()),flush=True)", "its.mean()),' max %d, >8: %.3f%%, fallbacks %d, unsolved %d'%(its.max(),100*(its>8).mean(),c.nfall,bad.sum()),flush=True)")
open('alias.py', 'w').write(s)
