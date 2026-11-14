// RAT gain stage (op-amp macromodel) + clipper + filter, implicit trapezoidal, Newton. Review prototype.
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <string.h>
#define N 6
static double It,Vl,Cc,Ro,Vr,Vk=0.05,Rf,Cf=100e-12,R1=560,C1=4.7e-6,R2=47,C2=2.2e-6;
static double Rs=1000,Rt=1500,Ct=3.3e-9,Is=2.52e-9,nVt=1.752*0.02585; static int dio=0;
static double idio(double v){ if(dio==2) return 0; return Is*(exp(v/nVt)-exp(-v/nVt)); }
// z = vc,w,v1,v2,vf,d ; f = derivatives (first 5), g = algebraic residual (6th)
static void rhs(const double*z,double u,double*f){
  double vc=z[0],w=z[1],v1=z[2],v2=z[3],vf=z[4],d=z[5];
  double out=vc, vm=out-w, vd=u-vm;
  double icl=It*1e-3*(exp((vc-Vr)/Vk)-exp((-vc-Vr)/Vk));
  f[0]=(It*tanh(vd/Vl)-vc/Ro-icl)/Cc;
  f[1]=((vm-v1)/R1+(vm-v2)/R2-w/Rf)/Cf;
  f[2]=(vm-v1)/(R1*C1); f[3]=(vm-v2)/(R2*C2);
  f[4]=(d-vf)/(Rt*Ct);
  f[5]=(out-d)/Rs-idio(d)-(d-vf)/Rt;
}
static int solve(double*a,double*b){ // gauss partial pivot NxN
  for(int c=0;c<N;c++){int p=c;for(int r=c+1;r<N;r++)if(fabs(a[r*N+c])>fabs(a[p*N+c]))p=r;
    if(p!=c){for(int k=0;k<N;k++){double t=a[c*N+k];a[c*N+k]=a[p*N+k];a[p*N+k]=t;}double t=b[c];b[c]=b[p];b[p]=t;}
    double dd=a[c*N+c]; if(fabs(dd)<1e-300) return 1;
    for(int r=c+1;r<N;r++){double m=a[r*N+c]/dd;for(int k=c;k<N;k++)a[r*N+k]-=m*a[c*N+k];b[r]-=m*b[c];}}
  for(int r=N-1;r>=0;r--){double s=b[r];for(int k=r+1;k<N;k++)s-=a[r*N+k]*b[k];b[r]=s/a[r*N+r];}
  return 0;}
static void resid(const double*z,const double*z0,const double*f0,double u,double h,double*g){
  double f[N]; rhs(z,u,f);
  for(int i=0;i<5;i++) g[i]=z[i]-z0[i]-0.5*h*(f[i]+f0[i]);
  g[5]=f[5]*1000; // volts-ish scaling
}
int main(int argc,char**argv){
  if(argc<12){fprintf(stderr,"os f0 amp Rf SR(V/us) GBW(Hz) dio(0si,1led,2none) mode(0 plain,1 limited) secs wave(0 sine,1 square) outfile [Rpot]\n");return 1;}
  int os=atoi(argv[1]); double f0=atof(argv[2]),amp=atof(argv[3]); Rf=atof(argv[4]);
  double SR=atof(argv[5])*1e6,GBW=atof(argv[6]); dio=atoi(argv[7]); int mode=atoi(argv[8]); double secs=atof(argv[9]); int wave=atoi(argv[10]);
  if(argc>12) Rt=1500+atof(argv[12]);
  if(dio==1){Is=93.2e-12;nVt=3.73*0.02585;}
  Cc=30e-12; It=SR*Cc; Vl=SR/(2*M_PI*GBW); Ro=3e5/(It/Vl); Vr=3.5;
  double fs=48000.0*os,h=1/fs; long n=(long)(secs*fs), settle=(long)(0.5*fs);
  double z[N]={0},zp[N]={0},zpp[N]={0},f0v[N]; FILE*fo=fopen(argv[11],"wb");
  double tol=getenv("TOL")?atof(getenv("TOL")):1e-9, lim0=0; double sc[N]={1,1,1,1,1,1}; long nres=0; long iters=0,maxit=0,nonconv=0; double maxslew=0,prevout=0; long hist[64]={0};
  for(long k=0;k<n+settle;k++){
    double t=k*h, ph=f0*t; double u= wave? (fmod(ph,1.0)<0.5?amp:-amp) : amp*sin(2*M_PI*ph);
    double tp=(k-1)*h, php=f0*tp; double up= k==0?0:(wave? (fmod(php,1.0)<0.5?amp:-amp) : amp*sin(2*M_PI*php));
    rhs(z,up,f0v); // f at previous point
    double z0[N]; memcpy(z0,z,sizeof z);
    double x[N]; for(int i=0;i<N;i++) x[i]=2*zp[i]-zpp[i]; // linear extrapolation seed (zp = z0)
    int it=0,ok=0; double g[N],J[N*N],gn;
    for(it=0;it<60;it++){
      resid(x,z0,f0v,u,h,g);
      for(int j=0;j<N;j++){double e=1e-7*(1+fabs(x[j])),xs=x[j],g2[N];x[j]=xs+e;resid(x,z0,f0v,u,h,g2);x[j]=xs;for(int i=0;i<N;i++)J[i*N+j]=(g2[i]-g[i])/e;}
      double dx[N]; for(int i=0;i<N;i++)dx[i]=-g[i];
      if(solve(J,dx)){break;}
      if(mode==1){ // device limiting: diode node step and op-amp differential input step
        double s=1; double lim=2*nVt; if(dio!=2 && fabs(x[5])>0.3 && fabs(dx[5])>lim) s=fmin(s,lim/fabs(dx[5]));
        // vd = u - vc + w : limit its change to 2 Vl
        double dvd=-dx[0]+dx[1]; if(fabs(dvd)>2*Vl) s=fmin(s,2*Vl/fabs(dvd));
        // integrator near rail clamp
        if(fabs(x[0]+dx[0])>Vr && fabs(dx[0])>4*Vk) s=fmin(s,fmax(4*Vk,fabs(Vr-fabs(x[0])))/fabs(dx[0]));
        for(int i=0;i<N;i++)dx[i]*=s;
      }
      if(mode>=2){ double n0=0; for(int i=0;i<N;i++) n0+=sc[i]*sc[i]*g[i]*g[i]; double s=1,xt[N],gt[N];
        if(mode==3){ double dvd=-dx[0]+dx[1]; if(fabs(dvd)>3*Vl) s=fmin(s,3*Vl/fabs(dvd)); if(dio!=2&&fabs(dx[5])>0.1&&x[5]*dx[5]>0&&fabs(x[5]+dx[5])>0.35) s=fmin(s,fmax(0.1,0.35-fabs(x[5]))/fabs(dx[5])*1.0+0*lim0); }
        for(int b=0;b<8;b++){ for(int i=0;i<N;i++)xt[i]=x[i]+s*dx[i]; resid(xt,z0,f0v,u,h,gt); double n1=0; for(int i=0;i<N;i++) n1+=sc[i]*sc[i]*gt[i]*gt[i]; nres++; if(n1<=n0*(1-0.2*s)||!(n0>0)) break; s*=0.5; }
        for(int i=0;i<N;i++)dx[i]*=s; }
      double m=0; for(int i=0;i<N;i++){x[i]+=dx[i]; if(fabs(dx[i])>m)m=fabs(dx[i]);}
      if(!(m==m)){break;}
      if(m<tol){ok=1;it++;break;}
    }
    if(!ok){nonconv++; memcpy(x,z0,sizeof x);} 
    iters+=it; if(it>maxit)maxit=it; hist[it<63?it:63]++;
    memcpy(zpp,zp,sizeof z); memcpy(zp,x,sizeof z); memcpy(z,x,sizeof z);
    if(k>=settle){ double sl=fabs(z[0]-prevout)/h; if(sl>maxslew)maxslew=sl; double o[2]={z[4],z[0]}; fwrite(o,8,2,fo);} prevout=z[0];
  }
  fclose(fo);
  fprintf(stderr,"extra resid evals/sample %.2f ",(double)nres/(n+settle)); fprintf(stderr,"mean it %.2f max %ld nonconv %ld maxslew %.3f V/us | hist",(double)iters/(n+settle),maxit,nonconv,maxslew*1e-6);
  for(int i=1;i<20;i++) if(hist[i]) fprintf(stderr," %d:%ld",i,hist[i]); fprintf(stderr,"\n");
  return 0;}
