import {createMdecInputDevice} from './mdec_input_device.mjs';

// Original movie MMIO boundary. Owners and callback phase are authenticated by
// the caller. Table upload stays in the existing SDK device; the first native
// command1 switches to the existing licensed functional frame decoder.
export function createFf9MovieMdecDevice({previous,ram,mem,platform,decoder,allowOutputDma=false,assertOwner,onTrace=()=>{}}={}){
 const demand=(c,s)=>{if(!c)throw Error('movie MDEC device: '+s);};
 demand(previous?.read32&&previous?.write32&&previous?.snapshot&&previous?.history&&ram?.buffer===mem?.buffer&&decoder?.decode&&typeof assertOwner==='function','initialized services and current owner check');
 let device=previous,inputDevice=false,transitions=0,reads=0,writes=0,failed=false;
 const i32=v=>Number.isInteger(v)&&v>=-2147483648&&v<=4294967295;
 const check=()=>{demand(!failed,'terminal device failure');demand(ram.buffer===mem.buffer,'shared memory drift');assertOwner();};
 const port=address=>{
  demand(i32(address),'exact i32 port');const a=(address>>>0)&0x1fffffff;
  demand(!(a&3),'aligned32 port');
  demand([0x1f801820,0x1f801824,0x1f8010f0,0x1f801080,0x1f801084,0x1f801088,0x1f801090,0x1f801094,0x1f801098].includes(a),'owned MDEC/DMA0/1 port');return a;
 };
 const read32=(...args)=>{
  demand(args.length===1,'exact read32 arity');check();port(args[0]);
  const before=device.history().length;let result;
  try{result=device.read32(args[0]);}catch(e){if(device.history().length!==before)failed=true;throw e;}
  demand(i32(result),'exact i32 bus result');reads++;return result;
 };
 const write32=(...args)=>{
  demand(args.length===2&&args.every(i32),'exact write32 i32 ABI');check();const a=port(args[0]),value=args[1]>>>0;
  let selected=device,transition=false;
  if(!inputDevice&&a===0x1f801820&&device.snapshot().remaining===0&&(value>>>29)===1){
   selected=createMdecInputDevice({previous:device,ram,mem,platform,decoder,allowOutputDma,onTrace});transition=true;
  }
  const before=selected.history().length;
  try{selected.write32(...args);}catch(e){if(selected.history().length!==before)failed=true;throw e;}
  // A rejected first command must preserve the original table device as well
  // as RAM/register/history state. Commit the switch only after acceptance.
  if(transition){device=selected;inputDevice=true;transitions++;}writes++;
 };
 Object.defineProperty(read32,'length',{value:1});Object.defineProperty(write32,'length',{value:2});
 return Object.freeze({read32,write32,snapshot:()=>device.snapshot(),history:()=>device.history(),decodedBytes:()=>device.decodedBytes?.()??null,
  isInputDevice:()=>inputDevice,diagnostics:()=>Object.freeze({inputDevice,transitions,reads,writes,failed,decodedFrames:device.snapshot().decodes?.length??0,outputDmas:device.snapshot().outputDmas?.length??0,pendingOutputBytes:device.snapshot().pendingOutputBytes??0,
   scope:'Command-driven table-to-frame transition; existing functional decoder/DMA dispatch. Original owner checks required. CPU FIFO packing and silicon/cycle equivalence remain unsupported.'})});
}
