import assert from 'node:assert/strict';
import fs from 'node:fs';
import crypto from 'node:crypto';
import {verifyResidentTableDerivation,verifyResidentFunctionDerivation,verifyResidentModuleIdentity,residentSourceBytes,residentDerivationIdentity} from '../runtime/resident_module_derivation.mjs';
const [originalPath,parentPath,tableProofPath,derivedPath,proofPath,outputPath]=process.argv.slice(2);
assert(outputPath&&[originalPath,parentPath,tableProofPath,derivedPath,proofPath].every(Boolean));
assert(!fs.existsSync(outputPath),'fresh output required');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),read=p=>new Uint8Array(fs.readFileSync(p));
const originalBytes=read(originalPath),parentBytes=read(parentPath),tableProofBytes=read(tableProofPath),derivedBytes=read(derivedPath),proofBytes=read(proofPath);
const parent=await verifyResidentTableDerivation({originalBytes,derivedBytes:parentBytes,proofBytes:tableProofBytes,expectedOriginalSha256:sha(originalBytes),expectedProofSha256:sha(tableProofBytes)});
const options={parentDerivation:parent,parentBytes,derivedBytes,proofBytes,expectedProofSha256:sha(proofBytes)};
const token=await verifyResidentFunctionDerivation(options),identity=await verifyResidentModuleIdentity(derivedBytes,sha(originalBytes),token);
assert(Object.isFrozen(token)&&Object.isFrozen(identity));assert.equal(identity,residentDerivationIdentity(token));
assert.equal(identity.parentModuleSha256,sha(parentBytes));assert.equal(identity.parentDerivationSha256,parent.derivationSha256);
assert.deepEqual(residentSourceBytes(token),originalBytes);assert.equal(identity.tableMaximum,parent.tableMaximum);
let refusals=0;
async function refuse(change,pattern){await assert.rejects(()=>verifyResidentFunctionDerivation({...options,...change}),pattern);refusals++;}
await refuse({parentDerivation:{...parent}},/opaque parent/);
for(const field of ['parentBytes','derivedBytes','proofBytes']){const bytes=options[field].slice();bytes[0]^=1;await refuse({[field]:bytes});}
await refuse({expectedProofSha256:'0'.repeat(64)},/parent\/proof identity/);
await assert.rejects(()=>verifyResidentModuleIdentity(parentBytes,sha(originalBytes),token));refusals++;
await assert.rejects(()=>verifyResidentModuleIdentity(derivedBytes,sha(originalBytes),{...token}));refusals++;
assert.throws(()=>residentSourceBytes({...token}));refusals++;
assert.throws(()=>residentDerivationIdentity({...token}));refusals++;
const base=JSON.parse(new TextDecoder().decode(proofBytes));
async function refuseProof(change,pattern,bytes=derivedBytes){
 const proof=new TextEncoder().encode(JSON.stringify({...base,...change}));
 await refuse({derivedBytes:bytes,proofBytes:proof,expectedProofSha256:sha(proof)},pattern);
}
for(const change of [{format:'private-patch'},{reader:'unreviewed'},{schemaVersion:2},{sourceModuleSha256:'0'.repeat(64)},{parentModuleSha256:'0'.repeat(64)},{parentDerivationSha256:'0'.repeat(64)},{moduleSha256:'0'.repeat(64)},{allNonCodeSectionsEqual:false},{importedFunctions:base.importedFunctions+1},{definedFunctions:base.definedFunctions+1},{changedFunctions:[]},{changedFunctions:base.changedFunctions.slice(1)},{changedFunctions:[...base.changedFunctions].reverse()},{changedFunctions:[...base.changedFunctions,base.changedFunctions[0]]}])await refuseProof(change);
await refuseProof({changedFunctions:base.changedFunctions.map((v,i)=>i? v:{...v,beforeBodySha256:'0'.repeat(64)})},/function bodies/);
await refuseProof({changedFunctions:base.changedFunctions.map((v,i)=>i? v:{...v,afterBodySha256:'0'.repeat(64)})},/function bodies/);
// A new custom section is valid Wasm, but outside the reviewed code-only scope.
const altered=new Uint8Array(derivedBytes.length+3);altered.set(derivedBytes);altered.set([0,1,0],derivedBytes.length);new WebAssembly.Module(altered);
await refuseProof({moduleSha256:sha(altered)},/non-code sections unchanged/,altered);
// Copies happen before digest awaits. Later caller mutations cannot change admission.
const isolatedOptions={...options,parentBytes:parentBytes.slice(),derivedBytes:derivedBytes.slice(),proofBytes:proofBytes.slice()};
const pending=verifyResidentFunctionDerivation(isolatedOptions);
for(const field of ['parentBytes','derivedBytes','proofBytes'])isolatedOptions[field][0]^=1;
const isolated=await pending;assert.deepEqual(residentDerivationIdentity(isolated),identity);
const copy=residentSourceBytes(token);copy[0]^=1;assert.deepEqual(residentSourceBytes(token),originalBytes);
const result={ok:true,identity,changedFunctions:base.changedFunctions.map(v=>v.index),refusals,opaqueTokens:true,inputMutationIsolation:true,sourceCopyIsolation:true,nonCodeMutationRefused:true,helperSha256:sha(fs.readFileSync(new URL('../runtime/resident_module_derivation.mjs',import.meta.url))),testSha256:sha(fs.readFileSync(new URL(import.meta.url))),newGuestCalls:0,scope:'Resident module lineage and exact code-body admission only; no startup, game, device, frame or PCM execution'};
fs.writeFileSync(outputPath,JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
