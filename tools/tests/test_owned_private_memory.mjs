import assert from 'node:assert/strict';
import {createOwnedPrivateMemoryView}from '../runtime/owned_private_memory.mjs';
import {createBiosMemoryHost}from '../../target/ff9-title-movie-critical-20261004/runtime/bios-memory-host.mjs';
const base=0x80000000,bytes=new Uint8Array(0x380000).fill(0xa5),ram={length:base+bytes.length,buffer:bytes.buffer,subarray:(a,b)=>bytes.subarray(a-base,b-base),set:(v,a)=>bytes.set(v,a-base),fill:(v,a,b)=>bytes.fill(v,a-base,b-base)};
let running=true,ownerValid=true;const allocation={base:0x80340000,end:0x80380000,stackBase:0x80360000,moduleSha256:'a'.repeat(64)},state={moduleSha256:allocation.moduleSha256,privateMemoryBase:allocation.base,privateMemoryEnd:allocation.end,privateStackBase:allocation.stackBase,privateStackPointer:0x8035ffe0};
const owner=Object.freeze({check:()=>assert(ownerValid,'invalid original owner'),snapshot:()=>({...state})}),view=createOwnedPrivateMemoryView({ram,owner,allocation,active:()=>running});
const traces=[],host=createBiosMemoryHost({ram,exports:{},kernelRam:view,onTrace:(...v)=>traces.push(v)});
const before=Buffer.from(bytes),expected=Buffer.from(bytes);expected.fill(0,0x35ffe8,0x35fff0);
assert.equal(host.memset(0x8035ffe8,0,8),0x8035ffe8);assert.deepEqual(Buffer.from(bytes),expected);assert.equal(view.normalize(0x8035ffe8),0x8035ffe8);
const copied=view.readBytes(0x8035ffe8,8);copied.fill(255);assert.deepEqual(view.readBytes(0x8035ffe8,8),new Uint8Array(8));
let refusals=0;
const refuse=(operation,reason)=>{const before=Buffer.from(bytes),trace=JSON.stringify(traces);assert.throws(operation,reason);assert.deepEqual(Buffer.from(bytes),before);assert.equal(JSON.stringify(traces),trace);refusals++;};
refuse(()=>host.memset(0x8035ffdf,0,2),/range/);refuse(()=>host.memset(0x8035ffff,0,2),/range/);refuse(()=>host.memset(0x80360000,0,1),/range/);
refuse(()=>view.writeBytes(0x8035ffe0,new Uint8Array(33)),/active owned stack/);refuse(()=>view.writeBytes(0x8035ffe0,[0]),/byte array/);
running=false;refuse(()=>host.memset(0x8035ffe8,0,8),/inactive/);running=true;
ownerValid=false;refuse(()=>host.memset(0x8035ffe8,0,8),/invalid original owner/);ownerValid=true;
for(const [key,value]of [['moduleSha256','b'.repeat(64)],['privateMemoryBase',0x80330000],['privateMemoryEnd',0x80390000],['privateStackBase',0x80370000],['privateStackPointer',0x8035ffe1]]){const old=state[key];state[key]=value;refuse(()=>host.memset(0x8035ffe8,0,8),/drift/);state[key]=old;}
assert.throws(()=>createOwnedPrivateMemoryView({ram,owner,allocation:{...allocation,end:0x80400000},active:()=>true}),/allocation/);
// Original guest byte semantics still work through the unchanged BIOS host.
expected.fill(0x23,0x100,0x107);assert.equal(host.memset(0x80000100,0x123,7),0x80000100);assert.deepEqual(Buffer.from(bytes),expected);
assert.deepEqual(Buffer.from(bytes.subarray(0x200000,0x340000)),before.subarray(0x200000,0x340000));
console.log(JSON.stringify({ok:true,wholeMemoryBytesCompared:bytes.length,noEffectRefusals:refusals,existingBiosMemoryHostReused:true,identityAddressesPreserved:true}));
