// Exact byte equality, including unaligned views and tails. No hash/cache trust.
export function exactBytesEqual(a,b){
 if(a.length!==b.length)return false;
 const n=a.length,av=new DataView(a.buffer,a.byteOffset,n),bv=new DataView(b.buffer,b.byteOffset,n);let i=0;
 for(;i+4<=n;i+=4)if(av.getUint32(i,true)!==bv.getUint32(i,true))return false;
 for(;i<n;i++)if(a[i]!==b[i])return false;
 return true;
}
