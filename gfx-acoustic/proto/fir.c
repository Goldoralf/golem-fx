#include <immintrin.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>
static inline float dot(const float*a,const float*b,int n){
  __m256 s0=_mm256_setzero_ps(),s1=s0,s2=s0,s3=s0;
  for(int i=0;i<n;i+=32){
    s0=_mm256_fmadd_ps(_mm256_loadu_ps(a+i),_mm256_loadu_ps(b+i),s0);
    s1=_mm256_fmadd_ps(_mm256_loadu_ps(a+i+8),_mm256_loadu_ps(b+i+8),s1);
    s2=_mm256_fmadd_ps(_mm256_loadu_ps(a+i+16),_mm256_loadu_ps(b+i+16),s2);
    s3=_mm256_fmadd_ps(_mm256_loadu_ps(a+i+24),_mm256_loadu_ps(b+i+24),s3);}
  s0=_mm256_add_ps(_mm256_add_ps(s0,s1),_mm256_add_ps(s2,s3));
  __m128 lo=_mm_add_ps(_mm256_castps256_ps128(s0),_mm256_extractf128_ps(s0,1));
  lo=_mm_hadd_ps(lo,lo);lo=_mm_hadd_ps(lo,lo);return _mm_cvtss_f32(lo);}
int main(){
  int Ns[]={1024,2048,4096};
  for(int q=0;q<3;q++){int N=Ns[q];
    float*h1=aligned_alloc(32,4*N),*h2=aligned_alloc(32,4*N),*hist=aligned_alloc(32,8*N);
    for(int i=0;i<N;i++){h1[i]=rand()/(float)RAND_MAX-.5f;h2[i]=rand()/(float)RAND_MAX-.5f;}
    for(int i=0;i<2*N;i++)hist[i]=0;
    for(int nk=1;nk<=2;nk++){
      long M=3000000; int pos=0; volatile float acc=0; struct timespec a,b;
      clock_gettime(CLOCK_MONOTONIC,&a);
      for(long n=0;n<M;n++){float x=(n&1023)*1e-3f; hist[pos]=x;hist[pos+N]=x; pos=(pos+1)&(N-1);
        float y=dot(hist+pos,h1,N); if(nk==2) y=0.5f*y+0.5f*dot(hist+pos,h2,N); acc+=y;}
      clock_gettime(CLOCK_MONOTONIC,&b);
      printf("N=%d kernels=%d: %.1f ns/sample\n",N,nk,((b.tv_sec-a.tv_sec)*1e9+(b.tv_nsec-a.tv_nsec))/M);}
  }return 0;}
