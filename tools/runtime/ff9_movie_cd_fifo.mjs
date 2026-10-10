import {createRawCdFifo} from './raw_cd_fifo.mjs';

// Original C434's typed void2 FIFO boundary. The caller must authenticate its
// movie module, callback registry, native aliases and active ReadS command in
// assertReady. This service owns bytes/cursors, never guest ring bookkeeping.
export function createFf9MovieCdFifo({memory,source,range,getStackPointer,assertReady}={}){
 const demand=(c,s)=>{if(!c)throw Error('FF9 movie CD FIFO: '+s);};
 demand(memory instanceof WebAssembly.Memory&&typeof getStackPointer==='function'&&typeof assertReady==='function','shared memory and current owner/phase services');
 const ram=new Uint8Array(memory.buffer),mem=new DataView(memory.buffer);
 demand(ram.length>=0x80380000,'guest/resident/movie private memory');
 const check=()=>{demand(memory.buffer===ram.buffer,'shared memory drift');assertReady();};
 const fifo=createRawCdFifo({source,range,assertContext:check});
 const word=a=>mem.getUint32(a,true);
 let ready=null;
 const beginReady=lba=>{
  check();demand(!ready,'ready callback already active');
  const sector=word(0x8019de5c),groupSize=word(0x8019de60),stack=getStackPointer()>>>0;
  demand(word(0x8019de54)===lba&&word(0x8019dea4)===0&&groupSize>2&&sector<groupSize,'original idle cursor/group');
  demand(stack>=0x80340010&&stack<=0x80360000&&!(stack&15),'owned callback entry stack');
  fifo.beginSector(lba);ready={lba,sector,groupSize,stack,metadata:sector<2,reads:0};
 };
 const CdGetSector=(...args)=>{
  check();demand(ready,'FIFO outside original ready callback');
  demand(args.length===2,'exact void2 FIFO arity');let [destination,words]=args;
  demand(Number.isInteger(destination)&&destination>=-2147483648&&destination<=4294967295&&Number.isInteger(words),'exact i32 FIFO arguments');
  destination>>>=0;const offset=fifo.snapshot().offset;
  if(ready.reads===0){
   demand(words===3&&offset===0&&destination===ready.stack-16&&(getStackPointer()>>>0)===destination,'original compiler header frame');
  }else{
   demand(ready.reads===1&&offset===12&&words===(ready.metadata?512:581),'ordered original metadata/payload read');
   const c=0x8019de10,w=off=>word(c+off);
   const expected=ready.metadata?w(0x90)+w(0x80)*2048:w(0x78)+(w(0x68)*w(0x5c)-(w(0x64)-(w(0x5c)-1)))*0x8f4-0x20;
   demand(Number.isSafeInteger(expected)&&expected>=0x80000000&&expected+words*4<=0x80200000&&destination===expected,'original selected ring destination');
  }
  const bytes=fifo.readWords(words);ram.set(bytes,destination);ready.reads++;
  // The compiled import is void2; original CdGetSector's result is discarded.
 };
 const finishReady=()=>{
  check();demand(ready&&ready.reads===2,'complete original FIFO callback');
  demand((getStackPointer()>>>0)===ready.stack&&word(0x8019de54)===ready.lba+1&&word(0x8019de5c)===(ready.sector+1)%ready.groupSize&&word(0x8019dea4)===0,'original cursor/stack return');
  const receipt=fifo.finishSector(ready.metadata?2060:2336);ready=null;return receipt;
 };
 const snapshot=()=>Object.freeze({...fifo.snapshot(),ready:ready?Object.freeze({...ready}):null});
 return Object.freeze({beginReady,CdGetSector,finishReady,snapshot});
}
