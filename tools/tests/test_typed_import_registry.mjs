import assert from 'node:assert/strict';import {createTypedImportRegistry} from '../runtime/typed_import_registry.mjs';
const i={module:'env',field:'query',kind:'function',parameters:['i32'],results:['i32']},linked={importInventory:[i,{module:'env',field:'memory',kind:'memory'}]};let calls=0;
const p={module:'env',field:'query',parameters:['i32'],results:['i32'],invoke:x=>{calls++;return x;}},r=createTypedImportRegistry(linked,[p]);assert(Object.isFrozen(r));assert.equal(r.size,1);assert.equal(r.get('env','query').length,1);assert.equal(r.get('env','missing'),null);assert.equal(r.get('other','query'),null);
for(const args of [[],[1,2],[0.5],[4294967296],[-2147483649]]){const before=calls;assert.throws(()=>r.get('env','query')(...args));assert.equal(calls,before);}
for(const x of [-2147483648,-1,0,2147483647,4294967295])assert.equal(r.get('env','query')(x),x);
p.invoke=()=>999;p.parameters.push('i32');assert.equal(r.get('env','query')(7),7);assert.equal(r.get('env','query').length,1);
for(const q of [{...p,field:'missing'},{...p,field:'memory'},{...p,parameters:[],results:['i32']},{...p,parameters:['i32'],results:[]}])assert.throws(()=>createTypedImportRegistry(linked,[q]));
assert.throws(()=>createTypedImportRegistry(linked,[{...p,parameters:['i32']},{...p,parameters:['i32']}]));assert.throws(()=>createTypedImportRegistry({importInventory:[i,i]},[]));
const invalid=createTypedImportRegistry(linked,[{...p,parameters:['i32'],invoke:()=>undefined}]);assert.throws(()=>invalid.get('env','query')(0),/provider result/);
const voidLinked={importInventory:[{...i,results:[]}]};let stored;const v=createTypedImportRegistry(voidLinked,[{...p,parameters:['i32'],results:[],invoke:x=>{stored=x;}}]);assert.equal(v.get('env','query')(12),undefined);assert.equal(stored,12);
console.log(JSON.stringify({ok:true,argumentRefusalsBeforeInvocation:5,unknownAndSignatureRefusals:true,providerDescriptorIsolation:true,resultChecks:true}));
