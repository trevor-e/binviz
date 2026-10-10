import assert from 'node:assert/strict';
import fs from 'node:fs';
import crypto from 'node:crypto';
import {createTypedCallbackRegistry,verifyTypedCallbackBinding} from '../runtime/typed_callback_registry.mjs';
// Uses a real shared-build wrapper, shared linked inventories and the actual
// resident resolver. The callback is a recording provider, not movie admission.
const [wrapperPath,linkedPath,residentPath,movieLinkedPath,outputPath,tableReviewPath]=process.argv.slice(2);
assert(wrapperPath&&linkedPath&&residentPath&&movieLinkedPath&&outputPath,'five fixture paths required');
const bytes=new Uint8Array(fs.readFileSync(wrapperPath)),linked=JSON.parse(fs.readFileSync(linkedPath)),movieLinked=JSON.parse(fs.readFileSync(movieLinkedPath));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const residentBytes=fs.readFileSync(residentPath);
const originalSha='3c3da705d6f7d71a8d34721eeba969cbda69dcd9b86dc8ec1d400c594237d400';
let tableReview;
if(tableReviewPath){
 tableReview=JSON.parse(fs.readFileSync(tableReviewPath));
 assert.equal(tableReview.beforeSha256,originalSha);assert.equal(sha(residentBytes),tableReview.afterSha256);
 const original=fs.readFileSync(tableReview.originalModulePath),{start,end}=tableReview.changedEncodingRange;
 assert.equal(sha(original),originalSha);assert.equal(original.length,residentBytes.length);
 assert(Number.isInteger(start)&&Number.isInteger(end)&&start>=8&&end>start&&end<=original.length);
 assert.deepEqual(original.subarray(0,start),residentBytes.subarray(0,start));assert.deepEqual(original.subarray(end),residentBytes.subarray(end));
 assert.equal(tableReview.minimum,3786);assert.equal(tableReview.beforeMaximum,3786);assert.equal(tableReview.afterMaximum,3850);
}else assert.equal(sha(residentBytes),originalSha);
const residentModule=new WebAssembly.Module(residentBytes),imports={};let accidentalImports=0;
for(const v of WebAssembly.Module.imports(residentModule)){
 assert.equal(v.kind,'function');
 (imports[v.module]??={})[v.name]=()=>{accidentalImports++;throw Error('unexpected resident host call '+v.module+'.'+v.name);};
}
const x=new WebAssembly.Instance(residentModule,imports).exports;
x.ff9_init_fnmap();
const memory=x.memory,table=x.__indirect_function_table,ram=new Uint8Array(memory.buffer),mem=new DataView(memory.buffer);
const BASE=0x80000000,END=0x80380000,KEY=0x802006e0,VAL=0x802106e0,N=16384,ADDRESS=0x8019c434;
// Exact non-pending ready fixture: the original routine owns the dispatch,
// masks the event to eight bits, and reads its native target from currentCmd.
ram[0x800761d2]=0;mem.setUint32(0x80076b28,ADDRESS,true);
const readySlot=x.ff9_resolve_u(0x800235a0)>>>0,ready=table.get(readySlot);
assert(readySlot>0&&ready?.length===20);
const invokeReady=value=>ready(value,0,...Array(18).fill(0));
const snapshot=()=>ram.slice(BASE,END),initial=snapshot(),oldEntries=Array.from({length:table.length},(_,i)=>table.get(i)),oldLength=table.length;
assert(oldLength>0);
const nativeClaim=(address,slot)=>{
 assert(slot>0&&slot===table.length);
 let free=0;
 for(let i=0;i<N;i++){
  const key=mem.getUint32(KEY+i*4,true);
  assert.notEqual(key,address,'native address already owned');assert.notEqual(key,slot,'fresh slot alias already owned');
  if(key===0)free++;
 }
 assert(free>=2,'native map capacity');
};
let owner=true,checks=0,claims=0,calls=[];
const options={memory,table,wrapperBytes:bytes,wrapperLinked:linked,registerAliases:(a,r,u)=>x.ff9_fnmap_add(a,r,u),resolve:a=>x.ff9_resolve_u(a)};
const registry=await createTypedCallbackRegistry(options);
assert(Object.isFrozen(registry));
const binding={address:ADDRESS,linked:movieLinked,exportName:'sub_8019c434',invoke:(...args)=>{assert.equal(args.length,1);calls.push(args[0]);},check:()=>{checks++;assert(owner,'movie owner inactive');},claim:(a,s)=>{claims++;nativeClaim(a,s);}};
const unchanged=()=>{assert.deepEqual(snapshot(),initial);assert.equal(table.length,oldLength);assert.deepEqual(Array.from({length:table.length},(_,i)=>table.get(i)),oldEntries);};
let refusals=0;
for(const change of [{address:0},{address:0x100000000},{exportName:'absent'},{exportName:'sub_8019c1b0'},{claim:()=>{throw Error('foreign native alias');}}]){
 assert.throws(()=>registry.bind({...binding,...change}));unchanged();refusals++;
}
owner=false;assert.throws(()=>registry.bind(binding));unchanged();refusals++;owner=true;
for(const change of [{wrapperBytes:bytes.map((v,i)=>i===0?v^1:v)},{wrapperLinked:{...linked,startFunction:1}},{wrapperLinked:{...linked,initializationWrites:[{}]}}]){
 await assert.rejects(()=>createTypedCallbackRegistry({...options,...change}));unchanged();refusals++;
}
const record=registry.bind(binding);
assert.equal(record.slot,oldLength);assert.equal(table.length,oldLength+1);assert(Object.isFrozen(record));
const receipt=verifyTypedCallbackBinding(registry,record);
assert(Object.isFrozen(receipt));assert.equal(receipt.memory,memory);assert.equal(receipt.table,table);assert.equal(receipt.binding,record);assert.equal(receipt.wrapperSha256,sha(bytes));
assert.equal(verifyTypedCallbackBinding(registry,record),receipt,'stable authenticated receipt');
const receiptBefore=snapshot(),ownedWrapper=table.get(record.slot);
for(const [candidateRegistry,candidateBinding] of [[{...registry},record],[registry,{...record}],[registry,undefined],[registry,{...record,address:ADDRESS+4}]]){
 assert.throws(()=>verifyTypedCallbackBinding(candidateRegistry,candidateBinding),/authentic .* capability/);
 assert.deepEqual(snapshot(),receiptBefore);assert.equal(table.length,oldLength+1);assert.equal(table.get(record.slot),ownedWrapper);refusals++;
}
table.set(record.slot,null);
assert.throws(()=>verifyTypedCallbackBinding(registry,record),/actual owned table entry/);
assert.deepEqual(snapshot(),receiptBefore);assert.equal(table.get(record.slot),null);refusals++;
table.set(record.slot,ownedWrapper);
assert.equal(verifyTypedCallbackBinding(registry,record),receipt);
assert.equal(x.ff9_resolve_u(ADDRESS)>>>0,record.slot);assert.equal(x.ff9_resolve_u(record.slot)>>>0,record.slot);
assert.deepEqual(Array.from({length:oldLength},(_,i)=>table.get(i)),oldEntries);
// Predict every registration write using the maintained native map's source
// and the shared linked disassembly constants, then compare all 3.5 MiB.
const expected=initial.slice(),ev=new DataView(expected.buffer),stores=[];
for(const key of [ADDRESS,record.slot,record.slot]){
 let h=(Math.imul(key,2654435761)>>>0)>>>18,probes=0;
 while(ev.getUint32(KEY-BASE+h*4,true)!==0&&ev.getUint32(KEY-BASE+h*4,true)!==key){h=(h+1)&16383;assert(++probes<N);}
 ev.setUint32(KEY-BASE+h*4,key,true);ev.setUint32(VAL-BASE+h*4,record.slot,true);
 stores.push({key,hashSlot:h,value:record.slot});
}
assert.deepEqual(snapshot(),expected,'only source-bound native alias stores');
const words=[-2147483648,-1,0,1,2147483647,4294967295],beforeCalls=snapshot();
for(const value of words){
 const before=calls.length;
 assert.equal(invokeReady(value),0);
 assert.equal(calls.length,before+1);assert.equal(calls.at(-1),value&255);
 const args=Array.from({length:20},(_,i)=>i===0?value:0x01020304+i);
 assert.equal(table.get(record.slot)(...args),0);assert.equal(calls.length,before+2);assert.equal(calls.at(-1),value|0);
}
assert.deepEqual(snapshot(),beforeCalls,'callback transport writes no memory');assert.equal(accidentalImports,0);
assert.throws(()=>registry.bind(binding));assert.equal(table.length,oldLength+1);assert.deepEqual(snapshot(),beforeCalls);refusals++;
// A throwing callback terminates its registry. Do not resume its WASM stack.
owner=false;assert.throws(()=>invokeReady(1),/movie owner inactive/);
assert.equal(registry.snapshot().failed,true);
assert.throws(()=>verifyTypedCallbackBinding(registry,record),/terminal callback registry/);
assert.throws(()=>registry.bind({...binding,address:ADDRESS+4}),/terminal callback registry/);
assert.deepEqual(snapshot(),beforeCalls);assert.equal(table.length,oldLength+1);
const out={ok:true,scope:'Real resident native resolver and non-pending 800235A0 -> internal ff9_icall_t1 dispatch to generic recording void1 provider; actual movie handler not executed',wrapperSha256:sha(bytes),residentSha256:sha(residentBytes),movieLinkedSha256:sha(fs.readFileSync(movieLinkedPath)),registrySha256:sha(fs.readFileSync(new URL('../runtime/typed_callback_registry.mjs',import.meta.url))),testSha256:sha(fs.readFileSync(new URL(import.meta.url))),tableReviewSha256:tableReviewPath?sha(fs.readFileSync(tableReviewPath)):null,initialTableLength:oldLength,readySlot,record,oldEntriesPreserved:oldLength,whole3_5MiBRegistrationEqual:true,registrationStores:stores,whole3_5MiBInvocationUnchanged:true,typedCalls:calls,wordCases:words.length,refusalsBeforeEffects:refusals,terminalOwnerFailure:true,checks,claims,accidentalImports};
fs.writeFileSync(outputPath,JSON.stringify(out,null,2)+'\n');console.log(JSON.stringify(out));
