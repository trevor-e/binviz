// Scoped state derivation for the original positive TITLE wait, using the
// owned functional reference clock. No silicon timing or waveform claim.
const demand=(c,s)=>{if(!c)throw Error('FF9 reference wait state: '+s);};
const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
function clockFields(audio,device){
 demand(audio?.phase==='after'&&!audio.failed&&audio.attempts===audio.accepted&&audio.accepted>0,'last completed owned clock quantum');
 const c=audio.clock,m=audio.mixer;
 demand(Number.isSafeInteger(c?.samples)&&c.samples>0&&Number.isSafeInteger(c.systemClocks)&&Number.isSafeInteger(c.remainderClocks)&&c.remainderClocks>=0&&c.remainderClocks<768,'explicit sample/system-clock domain');
 demand(c.systemClocks===c.samples*768+c.remainderClocks,'exact clock conversion');
 demand(m?.attached===true&&!m.audioFault&&m.sampleClock===c.samples,'same successful attached mixer');
 const cursor=(c.samples%512)*2,half=Math.floor(c.samples/256)%2;
 demand(m.state?.captureCursor===cursor&&m.captureHalf===half,'capture cursor derived from actual completed samples');
 demand(device.control===0xc081&&!device.irq&&!device.busy&&device.fifoHalfwords===0,'selected settled CD-input/reverb, no IRQ or transport profile');
 demand(same(audio.device,device),'no device mutation after last observed clock');
 // The reference clock calls device.step before sample conversion. The frozen
 // step applies CNT[5:0]; driver capture_pos starts at zero and advances by two.
 return{appliedControl:device.control&63,pendingControl:false,status:(device.control&63)|(half<<11)};
}
export function verifyFf9ReferenceWaitState({before,after,beforeAudio,afterAudio,referenceBefore,referenceAfter,trace,referenceTrace}={}){
 demand(before?.spu?.device&&after?.spu?.device&&referenceBefore?.spu?.device&&referenceAfter?.spu?.device,'complete native/source states');
 demand(Array.isArray(trace)&&same(trace,referenceTrace),'entire original ordered wait trace');
 demand(!trace.some(r=>r.type==='spu-write16'&&((r.address&0x1fffffff)===0x1f801daa)),'control mutation inside selected wait');
 const beforeFields=clockFields(beforeAudio,before.spu.device),afterFields=clockFields(afterAudio,after.spu.device);
 demand(afterAudio.clock.systemClocks>beforeAudio.clock.systemClocks&&afterAudio.clock.samples>beforeAudio.clock.samples,'positive original wait clocks');
 const expectedBefore=structuredClone(referenceBefore),expectedAfter=structuredClone(referenceAfter);
 Object.assign(expectedBefore.spu.device,beforeFields);Object.assign(expectedAfter.spu.device,afterFields);
 demand(same(before,expectedBefore),'all source-derived before state');
 demand(same(after,expectedAfter),'all source-derived after state');
 return Object.freeze({ok:true,beforeFields,afterFields,beforeSamples:beforeAudio.clock.samples,afterSamples:afterAudio.clock.samples,
  systemClockDelta:afterAudio.clock.systemClocks-beforeAudio.clock.systemClocks,sampleDelta:afterAudio.clock.samples-beforeAudio.clock.samples,
  entireOrderedTraceMatched:true,allOtherNativeSourceStateMatched:true,hardwareTimingVerified:false,audioWaveformVerified:false});
}
export function verifyFf9ReferencePointState({actual,reference,audio}={}){
 demand(actual?.spu?.device&&reference?.spu?.device,'complete point states');
 const old=reference.spu.device;
 demand(old.control===0xc081&&old.appliedControl===33&&old.pendingControl===true&&old.status===417,'selected frozen pre-mixer timing baseline');
 const fields=clockFields(audio,actual.spu.device),expected=structuredClone(reference);
 Object.assign(expected.spu.device,fields);
 demand(same(actual,expected),'all source-derived point state');
 return Object.freeze({ok:true,derivedFields:fields,samples:audio.clock.samples,systemClocks:audio.clock.systemClocks,
  allOtherNativeSourceStateMatched:true,hardwareTimingVerified:false,audioWaveformVerified:false});
}
