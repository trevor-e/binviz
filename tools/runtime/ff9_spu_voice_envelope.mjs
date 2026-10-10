// Existing FF9/Psy-Q28-byte getter contract, on the shared functional SPU.
// This reads ENVX; it does not implement or approximate active-voice ADSR.
const contracts=new WeakMap(),GETTER=0x8005931c,POINTER=0x800679e8;
const NATIVE_SHA='623bbd760d4adebe44f562bafb6ac696280a71f8942af3bd4f44241b10adcbba';
const require=(ok,message)=>{if(!ok)throw Error('SPU voice envelope: '+message)};
const digest=async bytes=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');
export async function verifyFf9SpuVoiceEnvelopeContract({nativeBytes,importDescriptor}){
 require(nativeBytes instanceof Uint8Array&&nativeBytes.length===28,'exact original getter extent');
 const bytes=nativeBytes.slice();require(await digest(bytes)===NATIVE_SHA,'original getter identity');
 require(importDescriptor?.module==='hle'&&importDescriptor.field==='SpuGetVoiceEnvelope'&&JSON.stringify(importDescriptor.parameters)==='["i32","i32","i32","i32","i32","i32"]'&&JSON.stringify(importDescriptor.results)==='["i32"]','existing six-word HLE import ABI');
 const token=Object.freeze({nativeGetterSha256:NATIVE_SHA,meaningfulArguments:2,source:'existing native getter; no new SDK body recovery'});contracts.set(token,bytes);return token;
}
export function createFf9SpuVoiceEnvelopeHost({ram,mem},spu,{contract,assertContext}={}){
 const original=contracts.get(contract);require(original,'verified getter contract required');
 require(ram?.subarray&&mem?.getUint32&&mem?.setUint16&&spu?.device?.read16&&spu.device.snapshot,'shared RAM and SPU device');
 require(typeof assertContext==='function','explicit caller context required');
 const validate=args=>{
  require(args.length===6&&args.every(Number.isInteger),'exact imported six-word arguments');
  // The28-byte original reads only a0/a1 and never SP or the remaining raw
  // incoming registers. Existing uniform thunk padding stays unconsumed.
  const voice=args[0]|0,out=args[1]>>>0;require(voice>=0&&voice<24,'selected24-voice domain');
  require(out===0x8007f79c+voice*8,'original voice-table halfword destination');
  require(ram.subarray(GETTER,GETTER+28).every((v,i)=>v===original[i]),'original getter code changed');
  const base=mem.getUint32(POINTER,true)>>>0;require((base&0x1fffffff)===0x1f801c00,'original shared SPU register base');
  assertContext();
  require(!(spu.device.snapshot().activeVoices&(1<<voice)),'active-voice ADSR requires a verified mixer');
  return {voice,out,port:(base+voice*16+12)>>>0};
 };
 const readEnvelope=(...args)=>{
  const {out,port}=validate(args),value=spu.device.read16(port)&65535;
  mem.setUint16(out,value,true);
  // Public source is void2; the existing uniform thunk is int6. Returning
  // the unsigned ENVX also preserves this original leaf's incidental v0.
  return value;
 };
 return {handlers:{SpuGetVoiceEnvelope:readEnvelope},validate,contract};
}
