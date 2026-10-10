import assert from 'node:assert/strict';import fs from 'node:fs';import crypto from 'node:crypto';
import {verifyResidentTableDerivation,verifyResidentModuleIdentity,residentSourceBytes,residentDerivationIdentity} from '../runtime/resident_module_derivation.mjs';
const [originalPath,derivedPath,proofPath,outputPath]=process.argv.slice(2);assert(originalPath&&derivedPath&&proofPath&&outputPath);
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const originalBytes=new Uint8Array(fs.readFileSync(originalPath)),derivedBytes=new Uint8Array(fs.readFileSync(derivedPath)),proofBytes=new Uint8Array(fs.readFileSync(proofPath));
const options={originalBytes,derivedBytes,proofBytes,expectedOriginalSha256:sha(originalBytes),expectedProofSha256:sha(proofBytes)};
const token=await verifyResidentTableDerivation(options),identity=await verifyResidentModuleIdentity(derivedBytes,options.expectedOriginalSha256,token);
assert(Object.isFrozen(token)&&Object.isFrozen(identity));assert.equal(residentDerivationIdentity(token),identity);assert.equal(identity.moduleSha256,sha(derivedBytes));assert.equal(identity.sourceModuleSha256,sha(originalBytes));
const fresh=residentSourceBytes(token);fresh[0]^=1;assert.deepEqual(residentSourceBytes(token),originalBytes);
let refusals=0;
for(const field of ['originalBytes','derivedBytes','proofBytes']){
 const b=options[field].slice();b[0]^=1;await assert.rejects(()=>verifyResidentTableDerivation({...options,[field]:b}));refusals++;
}
await assert.rejects(()=>verifyResidentModuleIdentity(originalBytes,options.expectedOriginalSha256,token));refusals++;
await assert.rejects(()=>verifyResidentModuleIdentity(derivedBytes,options.expectedOriginalSha256,{...token}));refusals++;
assert.throws(()=>residentSourceBytes({...token}));refusals++;
assert.throws(()=>residentDerivationIdentity({...token}));refusals++;
const base=JSON.parse(new TextDecoder().decode(proofBytes));
for(const change of [{allOtherBytesEqual:false},{changedEncodingRange:{start:0,end:2}},{format:'private-patch'},{afterMaximum:base.beforeMaximum},{minimum:base.beforeMaximum+1}]){
 const b=new TextEncoder().encode(JSON.stringify({...base,...change}));
 await assert.rejects(()=>verifyResidentTableDerivation({...options,proofBytes:b,expectedProofSha256:sha(b)}));refusals++;
}
// Even an authenticated declaration cannot hide a change to executable bytes.
const changed=derivedBytes.slice();changed[0x10000]^=1;
const forged=new TextEncoder().encode(JSON.stringify({...base,afterSha256:sha(changed)}));
await assert.rejects(()=>verifyResidentTableDerivation({...options,derivedBytes:changed,proofBytes:forged,expectedProofSha256:sha(forged)}),/all other module bytes unchanged/);refusals++;
// Inputs are copied before the first asynchronous digest yields.
const input={...options,originalBytes:originalBytes.slice(),derivedBytes:derivedBytes.slice(),proofBytes:proofBytes.slice()};
const pending=verifyResidentTableDerivation(input);input.originalBytes[0]^=1;input.derivedBytes[0]^=1;input.proofBytes[0]^=1;
const isolated=await pending;assert.deepEqual(residentSourceBytes(isolated),originalBytes);assert.deepEqual(residentDerivationIdentity(isolated),identity);
const direct=await verifyResidentModuleIdentity(originalBytes,options.expectedOriginalSha256);assert.equal(direct.derivationSha256,null);assert.equal(direct.moduleSha256,options.expectedOriginalSha256);
const out={ok:true,refusals,opaqueTokenCloneRefused:true,inputMutationIsolation:true,sourceCopyIsolation:true,identity,helperSha256:sha(fs.readFileSync(new URL('../runtime/resident_module_derivation.mjs',import.meta.url))),testSha256:sha(fs.readFileSync(new URL(import.meta.url))),scope:'Reviewed module derivation and immutable identity ownership; no game function or device execution'};
fs.writeFileSync(outputPath,JSON.stringify(out,null,2)+'\n');console.log(JSON.stringify(out));
