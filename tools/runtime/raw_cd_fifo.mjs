// Physical CD FIFO for synchronous workers. The caller verifies the disc profile,
// callback ownership and command phase; this service never advances guest state.
export function createRawCdFifo({source,range,assertContext}={}){
 const demand=(c,s)=>{if(!c)throw Error('raw CD FIFO: '+s);};
 demand(source&&Object.isFrozen(source)&&typeof source.read2340==='function','immutable physical source');
 demand(range&&Number.isSafeInteger(source.size)&&source.size>0&&source.size%2352===0&&source.size===range.sourceSize&&source.identitySha256===range.sourceIdentitySha256&&/^[a-f0-9]{64}$/.test(range.sourceIdentitySha256),'verified source identity');
 const {firstLba,sectorCount}=range;
 demand(Number.isSafeInteger(firstLba)&&firstLba>=0&&Number.isSafeInteger(sectorCount)&&sectorCount>0&&Number.isSafeInteger(firstLba+sectorCount)&&(firstLba+sectorCount)*2352<=source.size,'physical stream bounds');
 demand(typeof assertContext==='function','current owner/phase check');
 let active=null,offset=0,nextLba=firstLba,sectors=0,failed=false;
 const check=()=>{demand(!failed,'terminal source failure');assertContext();};
 const snapshot=()=>Object.freeze({nextLba,sectors,active:active!==null,activeLba:active?nextLba:null,offset,remainingBytes:active?2340-offset:0,failed});
 const beginSector=lba=>{
  check();demand(!active,'previous sector still active');
  demand(lba===nextLba&&sectors<sectorCount,'ordered physical stream bounds');
  let bytes;
  try{bytes=source.read2340(lba);demand(bytes instanceof Uint8Array&&bytes.length===2340,'exact physical2340 window');}
  catch(e){failed=true;throw e;}
  // A source may reuse its read buffer. Keep bytes stable throughout this event.
  active=bytes.slice();offset=0;return snapshot();
 };
 const readWords=words=>{
  check();demand(active,'read outside ready sector');
  demand(Number.isSafeInteger(words)&&words>0&&words<=585&&offset+words*4<=2340,'FIFO word bounds');
  const bytes=active.slice(offset,offset+words*4);offset+=bytes.length;return bytes;
 };
 const finishSector=expectedConsumedBytes=>{
  check();demand(active,'finish outside ready sector');
  demand(Number.isSafeInteger(expectedConsumedBytes)&&expectedConsumedBytes>=0&&expectedConsumedBytes<=2340&&expectedConsumedBytes%4===0&&offset===expectedConsumedBytes,'original callback consumption');
  const receipt=Object.freeze({lba:nextLba,consumedBytes:offset,discardedBytes:2340-offset});
  active=null;offset=0;nextLba++;sectors++;return receipt;
 };
 return Object.freeze({beginSector,readWords,finishSector,snapshot});
}
