// Browser-safe ownership of reviewed resident module derivations.
// The proof digest must come from the caller's reviewed, immutable input set.
const records=new WeakMap();
const demand=(c,s)=>{if(!c)throw Error('resident derivation: '+s);};
const sha=async b=>[...new Uint8Array(await crypto.subtle.digest('SHA-256',b))].map(v=>v.toString(16).padStart(2,'0')).join('');
const equal=(a,b)=>a.length===b.length&&a.every((v,i)=>v===b[i]);
const digest=s=>typeof s==='string'&&/^[0-9a-f]{64}$/.test(s);
// Engine validation happens first. This reader only separates section bytes and
// complete function bodies; it does not interpret or approve instructions.
function codeSections(bytes,module){
 const read=(cursor,end)=>{
  let value=0;
  for(let n=0;n<5;n++){
   demand(cursor.at<end,'truncated u32');const b=bytes[cursor.at++];
   demand(n!==4||(b&0xf0)===0,'u32 overflow');value+=(b&0x7f)*2**(n*7);
   if(!(b&0x80))return value;
  }
  throw Error('resident derivation: unterminated u32');
 };
 const sections=[],bodies=[];let found=false;const cursor={at:8};
 while(cursor.at<bytes.length){
  const start=cursor.at,id=bytes[cursor.at++],length=read(cursor,bytes.length),end=cursor.at+length;
  demand(end<=bytes.length,'section extent');sections.push({id,bytes:bytes.subarray(start,end)});
  if(id===10){
   demand(!found,'duplicate code section');found=true;const count=read(cursor,end);
   demand(count<=end-cursor.at,'bounded body count');
   for(let i=0;i<count;i++){
    const size=read(cursor,end),bodyEnd=cursor.at+size;
    demand(size>0&&bodyEnd<=end,'function body extent');bodies.push(bytes.subarray(cursor.at,bodyEnd));cursor.at=bodyEnd;
   }
   demand(cursor.at===end,'complete code section');
  }
  cursor.at=end;
 }
 demand(found,'code section required');
 return {sections,bodies,importedFunctions:WebAssembly.Module.imports(module).filter(v=>v.kind==='function').length};
}
export async function verifyResidentTableDerivation({originalBytes,derivedBytes,proofBytes,expectedOriginalSha256,expectedProofSha256}={}){
 demand([originalBytes,derivedBytes,proofBytes].every(b=>b instanceof Uint8Array),'byte inputs required');
 demand([expectedOriginalSha256,expectedProofSha256].every(s=>typeof s==='string'&&/^[0-9a-f]{64}$/.test(s)),'reviewed identity digests required');
 originalBytes=originalBytes.slice();derivedBytes=derivedBytes.slice();proofBytes=proofBytes.slice();
 const [original,derived,proofSha256]=await Promise.all([sha(originalBytes),sha(derivedBytes),sha(proofBytes)]);
 demand(original===expectedOriginalSha256&&proofSha256===expectedProofSha256,'original/proof identity');
 const proof=JSON.parse(new TextDecoder().decode(proofBytes)),{start,end}=proof.changedEncodingRange??{};
 demand(proof.format==='binviz-table-maximum-expansion'&&proof.schemaVersion===1&&proof.reader==='binviz-shared-wasm-reader'&&proof.tableIndex===0,'shared table proof');
 demand(proof.beforeSha256===original&&proof.afterSha256===derived&&proof.allOtherBytesEqual===true&&proof.allFileOffsetsPreserved===true,'derived module identity');
 demand(Number.isInteger(start)&&Number.isInteger(end)&&start>=8&&end>start&&end<=originalBytes.length&&originalBytes.length===derivedBytes.length&&proof.fileBytes===originalBytes.length,'exact preserved file extent');
 demand(equal(originalBytes.subarray(0,start),derivedBytes.subarray(0,start))&&equal(originalBytes.subarray(end),derivedBytes.subarray(end))&&!equal(originalBytes.subarray(start,end),derivedBytes.subarray(start,end)),'all other module bytes unchanged');
 demand([proof.minimum,proof.beforeMaximum,proof.afterMaximum].every(n=>Number.isInteger(n)&&n>=0&&n<=0xffffffff)&&proof.minimum<=proof.beforeMaximum&&proof.afterMaximum>proof.beforeMaximum,'bounded table growth');
 // Engine validation complements the shared reader without introducing a parser.
 new WebAssembly.Module(originalBytes);new WebAssembly.Module(derivedBytes);
 const identity=Object.freeze({moduleSha256:derived,sourceModuleSha256:original,derivationSha256:proofSha256,tableMinimum:proof.minimum,tableMaximum:proof.afterMaximum});
 const token=Object.freeze({id:'binviz-resident-table-derivation-v1',...identity});
 records.set(token,{originalBytes,identity});return token;
}
export async function verifyResidentFunctionDerivation({parentDerivation,parentBytes,derivedBytes,proofBytes,expectedProofSha256}={}){
 const parent=records.get(parentDerivation);demand(parent,'verified opaque parent derivation token');
 demand([parentBytes,derivedBytes,proofBytes].every(b=>b instanceof Uint8Array),'byte inputs required');
 demand(digest(expectedProofSha256),'reviewed proof identity digest required');
 parentBytes=parentBytes.slice();derivedBytes=derivedBytes.slice();proofBytes=proofBytes.slice();
 const [before,after,proofSha256]=await Promise.all([sha(parentBytes),sha(derivedBytes),sha(proofBytes)]);
 demand(before===parent.identity.moduleSha256&&proofSha256===expectedProofSha256,'parent/proof identity');
 const proof=JSON.parse(new TextDecoder().decode(proofBytes));
 demand(proof.format==='binviz-scoped-resident-code-derivation'&&proof.schemaVersion===1&&proof.reader==='browser-wasm-code-section-verifier-v1','scoped function proof');
 demand(proof.sourceModuleSha256===parent.identity.sourceModuleSha256&&proof.parentModuleSha256===before&&proof.parentDerivationSha256===parent.identity.derivationSha256&&proof.moduleSha256===after,'derived module lineage');
 demand(proof.allNonCodeSectionsEqual===true,'unchanged section declaration');
 const a=codeSections(parentBytes,new WebAssembly.Module(parentBytes)),b=codeSections(derivedBytes,new WebAssembly.Module(derivedBytes));
 demand(a.sections.length===b.sections.length&&a.sections.every((v,i)=>v.id===b.sections[i].id&&(v.id===10||equal(v.bytes,b.sections[i].bytes))),'all non-code sections unchanged');
 demand(a.importedFunctions===b.importedFunctions&&a.bodies.length===b.bodies.length&&proof.importedFunctions===a.importedFunctions&&proof.definedFunctions===a.bodies.length,'function inventory unchanged');
 const changes=proof.changedFunctions;demand(Array.isArray(changes)&&changes.length>0&&changes.length<=a.bodies.length,'explicit changed function inventory');
 let next=0;
 for(let i=0;i<a.bodies.length;i++)if(!equal(a.bodies[i],b.bodies[i])){
  const row=changes[next++];demand(row&&row.index===i+a.importedFunctions&&digest(row.beforeBodySha256)&&digest(row.afterBodySha256),'exact changed function indexes');
  const [oldBody,newBody]=await Promise.all([sha(a.bodies[i]),sha(b.bodies[i])]);
  demand(row.beforeBodySha256===oldBody&&row.afterBodySha256===newBody,'reviewed changed function bodies');
 }
 demand(next===changes.length,'no extra changed function declarations');
 const identity=Object.freeze({moduleSha256:after,sourceModuleSha256:parent.identity.sourceModuleSha256,derivationSha256:proofSha256,tableMinimum:parent.identity.tableMinimum,tableMaximum:parent.identity.tableMaximum,parentModuleSha256:before,parentDerivationSha256:parent.identity.derivationSha256});
 const token=Object.freeze({id:'binviz-resident-function-derivation-v1',...identity});
 records.set(token,{originalBytes:parent.originalBytes,identity});return token;
}
export async function verifyResidentModuleIdentity(bytes,expectedSourceSha256,derivation){
 demand(bytes instanceof Uint8Array,'resident identity byte input');
 const actual=await sha(bytes);
 if(derivation===undefined){demand(actual===expectedSourceSha256,'resident identity');return Object.freeze({moduleSha256:actual,sourceModuleSha256:actual,derivationSha256:null});}
 const v=records.get(derivation);demand(v,'verified opaque derivation token');
 demand(v.identity.sourceModuleSha256===expectedSourceSha256&&actual===v.identity.moduleSha256,'resident identity');return v.identity;
}
export function residentSourceBytes(derivation){
 const v=records.get(derivation);demand(v,'verified opaque derivation token');return v.originalBytes.slice();
}
export function residentDerivationIdentity(derivation){
 const v=records.get(derivation);demand(v,'verified opaque derivation token');return v.identity;
}
