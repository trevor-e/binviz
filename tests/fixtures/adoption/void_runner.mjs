import {readFileSync} from 'node:fs';
import {createInterface} from 'node:readline';
let exports,profile,variant;
for await(const line of createInterface({input:process.stdin})){
  const r=JSON.parse(line);if(r.op==='stop')break;
  if(r.op==='init'){
    ({instance:{exports}}=await WebAssembly.instantiate(readFileSync(r.artifacts['void-module'])));
    profile=r.profile.voidProfile;variant=r.profile.variant;
    console.log(JSON.stringify({ready:true,version:'actual-void-wasm-1',architecture:'wasm32'}));continue;
  }
  const arg=Number(BigInt(r.arguments[0]));const result=exports.void_fill(arg);
  if(result!==undefined)throw Error('linked void body returned a value');
  let state=exports.get_word();if(variant==='wrong-ram')state^=1;
  const data='0x'+[0,8,16,24].map(n=>((state>>>n)&255).toString(16).padStart(2,'0')).join('');
  const events=[{kind:'call',pc:'0x0',provider:'void_fill',profile:'real',arguments:[r.arguments[0]],delaySlot:false},{kind:'return',pc:'0x4',delaySlot:false}];
  if(variant==='extra-event')events.splice(1,0,{kind:'device',pc:'0x2',arguments:['0x1'],delaySlot:false});
  const execution={status:'executed',instructions:0,ram:[{id:'state',start:'0x0',data}],registers:{},events,checkpoints:{'after-void':{returned:true}},coverage:['void_fill'],voidProfile:profile};
  if(variant==='fake-zero')execution.returnWord='0x0';
  console.log(JSON.stringify({id:r.id,execution}));
}
