// A byte-preserving physical sector source, usable in browser workers and Node.
// Identity verification belongs to the caller supplying the source capability.
export function createRaw2352SectorSource({size,read,identitySha256}={}){
 const demand=(c,s)=>{if(!c)throw Error('raw CD source: '+s)};
 demand(Number.isSafeInteger(size)&&size>0&&size%2352===0,'whole2352-byte physical disc required');
 demand(typeof read==='function'&&typeof identitySha256==='string'&&/^[a-f0-9]{64}$/.test(identitySha256),'verified source identity/read capability required');
 const sectorCount=size/2352;
 const read2352=lba=>{
  demand(Number.isInteger(lba)&&lba>=0&&lba<sectorCount,'physical sector bounds');
  const bytes=read(lba*2352,2352);
  demand(bytes instanceof Uint8Array&&bytes.length===2352,'exact2352-byte read result');
  return new Uint8Array(bytes);
 };
 const read2340=lba=>read2352(lba).slice(12);
 return Object.freeze({size,sectorCount,identitySha256,read2352,read2340});
}
