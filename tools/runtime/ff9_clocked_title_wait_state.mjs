// Original C0C1 TITLE waits on the owned licensed software mixer clock.
// Complete native/source state and ordered trace remain explicit proof inputs.
const demand=(c,s)=>{if(!c)throw Error('FF9 clocked TITLE state: '+s);};
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
function capture(audio,mixer){
 demand(audio?.phase==='after'&&!audio.failed&&audio.attempts===audio.accepted&&audio.accepted>0,'successful owned clock observation');
 const c=audio.clock;
 demand(Number.isSafeInteger(c?.samples)&&c.samples>0&&Number.isSafeInteger(c.systemClocks)&&Number.isSafeInteger(c.remainderClocks)&&c.remainderClocks>=0&&c.remainderClocks<768&&c.systemClocks===c.samples*768+c.remainderClocks,'exact sample/system-clock conversion');
 const half=Math.floor(c.samples/256)%2,cursor=(c.samples%512)*2;
 for(const m of[audio.mixer,mixer])demand(m?.attached===true&&!m.audioFault&&m.sampleClock===c.samples&&m.captureHalf===half&&m.state?.captureCursor===cursor,'live and observed mixer capture history');
 return half;
}
function profile(s){const d=s?.spu?.device;demand(d&&d.control===0xc0c1&&!d.irq&&!d.busy&&d.fifoHalfwords===0,'selected C0C1, no IRQ/busy/manual FIFO checkpoint');return d;}
function original(s){const d=profile(s);demand(d.appliedControl===33&&d.pendingControl===true&&d.status===417,'exact frozen original timing baseline');return d;}
const settled=half=>({appliedControl:1,pendingControl:false,status:1|(half<<11)});
function compare(actual,reference,fields){original(reference);profile(actual);const expected=structuredClone(reference);Object.assign(expected.spu.device,fields);demand(same(actual,expected),'every other native/source state field');}
export function verifyFf9ClockedTitlePoint({actual,reference,audio,mixer}={}){
 const half=capture(audio,mixer),fields=settled(half);demand(same(audio.device,actual?.spu?.device),'unchanged device since last successful quantum');compare(actual,reference,fields);
 return Object.freeze({ok:true,derivedFields:fields,samples:audio.clock.samples,systemClocks:audio.clock.systemClocks,allOtherNativeSourceStateMatched:true,hardwareTimingVerified:false,audioWaveformVerified:false});
}
export function verifyFf9ClockedTitleWait({before,after,referenceBefore,referenceAfter,beforeAudio,afterAudio,beforeMixer,afterMixer,trace,referenceTrace,beforeMode,transferAdmission}={}){
 demand(Array.isArray(trace)&&same(trace,referenceTrace),'entire original ordered wait trace');
 demand(!trace.some(r=>r.type==='spu-irq'||r.type==='spu-dma-write-complete'||r.type==='spu-dma-read-complete'||r.type==='spu-pio-complete'||(r.type==='spu-write16'&&(r.address&0x1fffffff)===0x1f801daa)),'no CNT/transport/IRQ transition inside selected wait');
 const beforeHalf=capture(beforeAudio,beforeMixer),afterHalf=capture(afterAudio,afterMixer);let beforeFields;
 if(beforeMode==='ring-pending'){
  const a=transferAdmission;
  demand(a?.ok&&a.id==='second-frame-output-return'&&a.control===0xc0c1&&a.samples===beforeAudio.clock.samples&&a.orderedEvents>0&&a.allOtherNativeSourceStateMatched&&!a.hardwareTimingVerified,'verified ring checkpoint at same clock');
  beforeFields={appliedControl:33,pendingControl:true,status:417|(beforeHalf<<11)};
  demand(same(a.derivedFields,beforeFields),'pending CNT fields derived by original ordered ring history');
 }else{
  demand(beforeMode==='settled','explicit proved wait-entry mode');beforeFields=settled(beforeHalf);demand(same(beforeAudio.device,before?.spu?.device),'settled entry unchanged since last quantum');
 }
 demand(afterAudio.clock.systemClocks>beforeAudio.clock.systemClocks&&afterAudio.clock.samples>beforeAudio.clock.samples&&afterAudio.accepted>beforeAudio.accepted,'positive completed wait clock history');
 const afterFields=settled(afterHalf);demand(same(afterAudio.device,after?.spu?.device),'return device from last successful quantum');
 compare(before,referenceBefore,beforeFields);compare(after,referenceAfter,afterFields);
 return Object.freeze({ok:true,beforeFields,afterFields,beforeSamples:beforeAudio.clock.samples,afterSamples:afterAudio.clock.samples,sampleDelta:afterAudio.clock.samples-beforeAudio.clock.samples,systemClockDelta:afterAudio.clock.systemClocks-beforeAudio.clock.systemClocks,clockQuanta:afterAudio.accepted-beforeAudio.accepted,orderedTraceEntries:trace.length,entireOrderedTraceMatched:true,allOtherNativeSourceStateMatched:true,hardwareTimingVerified:false,audioWaveformVerified:false});
}
