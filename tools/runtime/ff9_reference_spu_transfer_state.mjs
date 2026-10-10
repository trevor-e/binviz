// Functional reference timing through the original TITLE ring transfer.
// The caller keeps the original native RAM, controller and ordered-trace gates.
import {verifyFf9ReferencePointState} from './ff9_reference_wait_state.mjs';
const demand=(ok,s)=>{if(!ok)throw Error('FF9 reference SPU transfer state: '+s);};
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
const controls=new Set([0xc081,0xc0a1,0xc0c1]);
const status=m=>(m.half<<11)|m.applied|(((m.applied>>>4)&3)===2?0x180:0);
const settle=m=>{if(m.pending){m.applied=m.control&63;m.pending=false;}};
function replay(events,half,applied,pending,validateReads){
 demand(Array.isArray(events),'explicit ordered events');
 const m={control:0xc081,half,applied,pending};
 for(const e of events){
  demand(e&&typeof e.type==='string','typed ordered event');
  if(e.type==='spu-irq'||e.type==='spu-pio-complete'||e.type==='spu-dma-read-complete')demand(false,'unsupported IRQ/manual/download transition');
  if(e.type==='spu-write16'||e.type==='spu-read16'){
   demand(Number.isInteger(e.address)&&e.address>=0&&e.address<=0xffffffff&&Number.isInteger(e.value)&&e.value>=0&&e.value<=65535,'exact halfword event');
   const a=e.address&0x1fffffff;
   demand(a>=0x1f801c00&&a<0x1f801e00&&(a&1)===0&&a!==0x1f801da8,'selected register-only transfer prefix');
   if(e.type==='spu-write16'&&a===0x1f801daa){demand(controls.has(e.value),'selected original ring CNT values');m.control=e.value;m.pending=true;}
   if(e.type==='spu-read16'&&a===0x1f801daa)demand(e.value===m.control,'ordered CNT read');
   if(e.type==='spu-read16'&&a===0x1f801dae){if(validateReads)demand(e.value===status(m),'pre-step status read');settle(m);}
  }
  if(e.type==='spu-dma-write-complete'){
   demand(Number.isInteger(e.start)&&Number.isInteger(e.end)&&Number.isInteger(e.bytes)&&e.start>=0&&e.start<524288&&e.bytes>0&&e.bytes<=524288&&e.end===(e.start+e.bytes)%524288,'bounded complete DMA upload');
   settle(m);demand(((m.applied>>>4)&3)===2,'DMA applies write-mode CNT before upload');
  }
 }
 return m;
}
export function createFf9ReferenceSpuTransferState({actual,reference,audio}={}){
 const origin=verifyFf9ReferencePointState({actual,reference,audio});
 const saved=structuredClone({clock:audio.clock,attempts:audio.attempts,accepted:audio.accepted});
 const half=Math.floor(saved.clock.samples/256)%2;
 return Object.freeze({
  verify({actual,reference,audio,mixer,trace}={}){
   demand(audio?.phase==='after'&&!audio.failed&&same({clock:audio.clock,attempts:audio.attempts,accepted:audio.accepted},saved),'no unobserved audio clock advance inside transfer proof');
   demand(mixer?.attached===true&&!mixer.audioFault&&mixer.sampleClock===saved.clock.samples&&mixer.captureHalf===half&&mixer.state?.captureCursor===(saved.clock.samples%512)*2,'same live mixer clock and capture cursor');
   demand(actual?.spu?.device&&reference?.spu?.device,'complete original/source states');
   const old=replay(trace,0,33,true,false),current=replay(trace,half,origin.derivedFields.appliedControl,origin.derivedFields.pendingControl,true);
   const legacy=reference.spu.device,device=actual.spu.device;
   demand(!legacy.irq&&!legacy.busy&&legacy.fifoHalfwords===0&&!device.irq&&!device.busy&&device.fifoHalfwords===0,'no IRQ/busy/manual FIFO in selected transfer checkpoints');
   demand(legacy.control===old.control&&legacy.appliedControl===old.applied&&legacy.pendingControl===old.pending&&legacy.status===status(old),'original timing baseline agrees with ordered source transitions');
   const fields={appliedControl:current.applied,pendingControl:current.pending,status:status(current)};
   const expected=structuredClone(reference);Object.assign(expected.spu.device,fields);
   demand(device.control===current.control&&same(actual,expected),'all source-derived transfer state');
   return Object.freeze({ok:true,derivedFields:fields,control:current.control,samples:saved.clock.samples,orderedEvents:trace.length,allOtherNativeSourceStateMatched:true,hardwareTimingVerified:false,audioWaveformVerified:false});
  }
 });
}
