#include <stdio.h>
#include <stdlib.h>
#include <time.h>
static float dot(const float*a,const float*b,int n){float s0=0,s1=0,s2=0,s3=0,s4=0,s5=0,s6=0,s7=0;
 #pragma omp simd reduction(+:s0)
 for(int i=0;i<n;i++) s0+=a[i]*b[i]; return s0+s1+s2+s3+s4+s5+s6+s7;}
int main(int argc,char**argv){int nt=atoi(argv[1]); int fs=atoi(argv[2]); int N=fs*10;
 float*x=malloc(sizeof(float)*(N+nt)); float*h1=malloc(sizeof(float)*nt),*h2=malloc(sizeof(float)*nt);
 for(int i=0;i<N+nt;i++)x[i]=(float)rand()/RAND_MAX-0.5f; for(int i=0;i<nt;i++){h1[i]=1.0f/(i+1);h2[i]=0.9f/(i+1);}
 struct timespec t0,t1; clock_gettime(CLOCK_MONOTONIC,&t0); float acc=0;
 for(int n=0;n<N;n++){float l=(n&255)/256.0f; acc+=(1-l)*dot(x+n,h1,nt)+l*dot(x+n,h2,nt);}
 clock_gettime(CLOCK_MONOTONIC,&t1); double dt=(t1.tv_sec-t0.tv_sec)+1e-9*(t1.tv_nsec-t0.tv_nsec);
 printf("taps %d fs %d: %.2f %% of one core (%.1f ns/sample) acc %g\n",nt,fs,dt/10*100,dt/N*1e9,acc); return 0;}
