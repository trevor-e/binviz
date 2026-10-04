import {readFileSync} from 'node:fs';import {createInterface} from 'node:readline';
let exports,events;
for await(const line of createInterface({input:process.stdin})){
 const r=JSON.parse(line);if(r.op==='stop')break;
 if(r.op==='init'){
  ({instance:{exports}}=await WebAssembly.instantiate(readFileSync(r.artifacts[r.profile.moduleArtifact]),{env:{runtime_vblank:n=>{events.push({kind:'device',pc:'0x4',provider:'runtime_vblank',profile:'controlled',arguments:['0x'+(n>>>0).toString(16)],delaySlot:false});return(n+1)|0;}}}));
  console.log(JSON.stringify({ready:true,version:'actual-service-fixture-1',architecture:'wasm32'}));continue;
 }
 events=[{kind:'call',pc:'0x0',provider:'poll',profile:'real',arguments:r.arguments,delaySlot:false}];const result=exports.poll(Number(BigInt(r.arguments[0])));events.push({kind:'return',pc:'0x8',delaySlot:false});
 console.log(JSON.stringify({id:r.id,execution:{status:'executed',instructions:0,returnWord:'0x'+(result>>>0).toString(16),ram:[],registers:{},events,checkpoints:{'after-service':{events:events.length}},coverage:['poll','runtime_vblank']}}));
}
