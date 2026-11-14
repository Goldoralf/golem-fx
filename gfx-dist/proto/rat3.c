// As rat.c, with: Armijo damping, cap per attempt, continuation fallback (blend the previous sample's problem
// into the current one in 4 then 16 steps). Reports total Newton iterations per sample incl. fallbacks.
#include <stdio.h>
#include <stdlib.h>
#include <math.h>
#include <string.h>
#define N 6
static double It,Vl,Cc,Ro,Vr,Vk=0.05,Rf,Cf=100e-12,R1=560,C1=4.7e-6,R2=47,C2=2.2e-6;
static double Rs=1000,Rt=1500,Ct=3.3e-9,Is=2.52e-9,nVt=1.752*0.02585; static int dio=0; static double tol=1e-6; static int CAP=10;
static double idio(double v){ if(dio==2) return 0; return Is*(exp(v/nVt)-exp(-v/nVt)); }
static void rhs(const double*z,double u,double*f){
  double vc=z[0],w=z[1],v1=z[2],v2=z[3],vf=z[4],d=z[5]; double out=vc, vm=out-w, vd=u-vm;
  double icl=It*1e-3*(exp((vc-Vr)/Vk)-exp((-vc-Vr)/Vk));
  f[0]=(It*tanh(vd/Vl)-vc/Ro-icl)/Cc; f[1]=((vm-v1)/R1+(vm-v2)/R2-w/Rf)/Cf;
  f[2]=(vm-v1)/(R1*C1); f[3]=(vm-v2)/(R2*C2); f[4]=(d-vf)/(Rt*Ct); f[5]=(out-d)/Rs-idio(d)-(d-vf)/Rt; }
static int solve(double*a,double*b){
  for(int c=0;c<N;c++){int p=c;for(int r=c+1;r<N;r++)if(fabs(a[r*N+c])>fabs(a[p*N+c]))p=r;
    if(p!=c){for(int k=0;k<N;k++){double t=a[c*N+k];a[c*N+k]=a[p*N+k];a[p*N+k]=t;}double t=b[c];b[c]=b[p];b[p]=t;}
    double dd=a[c*N+c]; if(fabs(dd)<1e-300) return 1;
    for(int r=c+1;r<N;r++){double m=a[r*N+c]/dd;for(int k=c;k<N;k++)a[r*N+k]-=m*a[c*N+k];b[r]-=m*b[c];}}
  for(int r=N-1;r>=0;r--){double s=b[r];for(int k=r+1;k<N;k++)s-=a[r*N+k]*b[k];b[r]=s/a[r*N+r];} return 0;}
static void resid(const double*z,const double*z0,const double*f0,double u,double h,double*g){
  double f[N]; rhs(z,u,f); for(int i=0;i<5;i++) g[i]=(z[i]-z0[i]-0.5*h*(f[i]+f0[i])); g[5]=f[5]*1000; }
static long its;
static int newton(double*x,const double*z0,const double*f0,double u,double h,int cap){
  double g[N],J[N*N];
  for(int it=0;it<cap;it++){ its++;
    resid(x,z0,f0,u,h,g);
    for(int j=0;j<N;j++){double e=1e-7*(1+fabs(x[j])),xs=x[j],g2[N];x[j]=xs+e;resid(x,z0,f0,u,h,g2);x[j]=xs;for(int i=0;i<N;i++)J[i*N+j]=(g2[i]-g[i])/e;}
    double dx[N]; for(int i=0;i<N;i++)dx[i]=-g[i]; if(solve(J,dx)) return 0;
    double n0=0; for(int i=0;i<N;i++) n0+=g[i]*g[i]; double s=1,xt[N],gt[N];
    for(int b=0;b<6;b++){ for(int i=0;i<N;i++)xt[i]=x[i]+s*dx[i]; resid(xt,z0,f0,u,h,gt); double n1=0; for(int i=0;i<N;i++) n1+=gt[i]*gt[i]; if(n1<=n0*(1-0.2*s)||!(n0>0)) break; s*=0.5; }
    double m=0; for(int i=0;i<N;i++){x[i]+=s*dx[i]; if(fabs(s*dx[i])>m)m=fabs(s*dx[i]);}
    if(!(m==m)) return 0; if(m<tol) return 1; }
  return 0; }
int main(int argc,char**argv){
  if(getenv("TOL")) tol=atof(getenv("TOL")); int os=atoi(argv[1]); double f0=atof(argv[2]),amp=atof(argv[3]); Rf=atof(argv[4]);
  double SR=atof(argv[5])*1e6,GBW=atof(argv[6]); dio=atoi(argv[7]); CAP=atoi(argv[8]); double secs=atof(argv[9]); int wave=atoi(argv[10]);
  if(argc>12) Rt=1500+atof(argv[12]); if(argc>13) Rs=atof(argv[13]);
  if(dio==1){Is=93.2e-12;nVt=3.73*0.02585;}
  Cc=30e-12; It=SR*Cc; Vl=SR/(2*M_PI*GBW); Ro=3e5/(It/Vl); Vr=3.5;
  double fs=48000.0*os,h=1/fs; long n=(long)(secs*fs), settle=(long)(0.5*fs);
  double z[N]={0},zp[N]={0},zpp[N]={0},f0v[N],f0old[N]={0},z0old[N]={0},uold=0; FILE*fo=fopen(argv[11],"wb");
  long tot=0,maxit=0,fail=0,fb1=0,fb2=0; double maxslew=0,prevout=0; long hist[200]={0};
  for(long k=0;k<n+settle;k++){
    double t=k*h, ph=f0*t; double u= wave? (fmod(ph,1.0)<0.5?amp:-amp) : amp*sin(2*M_PI*ph);
    double tp=(k-1)*h, php=f0*tp; double up= k==0?0:(wave? (fmod(php,1.0)<0.5?amp:-amp) : amp*sin(2*M_PI*php));
    rhs(z,up,f0v); double z0[N]; memcpy(z0,z,sizeof z);
    double x[N]; for(int i=0;i<N;i++) x[i]=2*zp[i]-zpp[i];
    its=0; int ok=newton(x,z0,f0v,u,h,CAP);
    for(int steps=4; !ok && steps<=64; steps*=4){ if(steps==4)fb1++; else fb2++;
      memcpy(x,z0,sizeof x); ok=1;
      for(int s=1;s<=steps&&ok;s++){ double a=(double)s/steps,zb[N],fb[N]; for(int i=0;i<N;i++){zb[i]=(1-a)*z0old[i]+a*z0[i]; fb[i]=(1-a)*f0old[i]+a*f0v[i];}
        ok=newton(x,zb,fb,(1-a)*uold+a*u,h,CAP);} }
    if(!ok){fail++; memcpy(x,z0,sizeof x);} 
    tot+=its; if(its>maxit)maxit=its; hist[its<199?its:199]++;
    memcpy(z0old,z0,sizeof z); memcpy(f0old,f0v,sizeof z); uold=u;
    memcpy(zpp,zp,sizeof z); memcpy(zp,x,sizeof z); memcpy(z,x,sizeof z);
    if(k>=settle){ double sl=fabs(z[0]-prevout)/h; if(sl>maxslew)maxslew=sl; double o[2]={z[4],z[0]}; fwrite(o,8,2,fo);} prevout=z[0];
  }
  fclose(fo); long c=0,p999=0; for(int i=0;i<200;i++){c+=hist[i]; if(c>=0.999*(n+settle)){p999=i;break;}}
  fprintf(stderr,"mean %.2f p99.9 %ld max %ld | fallback4 %ld fallback16+ %ld unsolved %ld | slew %.3f\n",(double)tot/(n+settle),p999,maxit,fb1,fb2,fail,maxslew*1e-6);
  return 0;}
