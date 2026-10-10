// Explicit typed callbacks in fresh slots of an existing universal WASM table.
// Caller owns the native alias implementation, immutable owner and phase proof.
const registries=new WeakMap();
export function verifyTypedCallbackBinding(registry,binding){
 const state=registries.get(registry);
 if(!state)throw Error('typed callback registry: authentic registry capability');
 const entry=state.records.get(binding?.address);
 if(!entry||entry.binding!==binding)throw Error('typed callback registry: authentic binding capability');
 if(state.failed())throw Error('typed callback registry: terminal callback registry');
 if(state.table.get(binding.slot)!==entry.wrapper)throw Error('typed callback registry: actual owned table entry');
 return entry.receipt;
}
export async function createTypedCallbackRegistry({memory,table,wrapperBytes,wrapperLinked,registerAliases,resolve}={}){
 const demand=(c,s)=>{if(!c)throw Error('typed callback registry: '+s);};
 demand(memory instanceof WebAssembly.Memory&&table instanceof WebAssembly.Table&&typeof registerAliases==='function'&&typeof resolve==='function','actual memory/table and native alias services');
 demand(wrapperBytes instanceof Uint8Array&&wrapperLinked&&Array.isArray(wrapperLinked.importInventory)&&Array.isArray(wrapperLinked.functions),'shared linked wrapper report and bytes');
 wrapperBytes=wrapperBytes.slice();wrapperLinked=structuredClone(wrapperLinked);
 const sha=[...new Uint8Array(await crypto.subtle.digest('SHA-256',wrapperBytes))].map(v=>v.toString(16).padStart(2,'0')).join('');
 demand(sha===wrapperLinked.sha256&&!wrapperLinked.problems?.length&&!wrapperLinked.initializationWrites?.length&&!wrapperLinked.tableInitializers?.length&&wrapperLinked.startFunction===null,'immutable wrapper identity/ownership');
 const inventory=wrapperLinked.importInventory,target=inventory.find(i=>i.module==='env'&&i.field==='ff9_callback_void1'),fn=wrapperLinked.functions.find(f=>f.exports?.includes('ff9_callback_u20'));
 demand(inventory.length===2&&inventory.some(i=>i.module==='env'&&i.field==='memory'&&i.kind==='memory')&&target?.kind==='function'&&JSON.stringify(target.parameters)==='["i32"]'&&JSON.stringify(target.results)==='[]','exact wrapper memory and void1 import');
 demand(fn&&!fn.imported&&fn.parameters.length===20&&fn.parameters.every(t=>t==='i32')&&JSON.stringify(fn.results)==='["i32"]'&&!fn.indirectCalls?.length&&fn.calls.length===1&&fn.calls[0].target===target.index,'actual universal20 wrapper export');
 const module=new WebAssembly.Module(wrapperBytes),actual=WebAssembly.Module.imports(module);
 demand(actual.length===inventory.length&&actual.every((v,i)=>v.module===inventory[i].module&&v.name===inventory[i].field&&v.kind===inventory[i].kind),'actual wrapper import inventory');
 const records=new Map();let failed=false;
 const bind=({address,linked,exportName,invoke,check,claim}={})=>{
  demand(!failed,'terminal callback registry');demand(Number.isInteger(address)&&address>0&&address<=0xffffffff&&!records.has(address),'unique native address');
  demand(typeof invoke==='function'&&typeof check==='function'&&typeof claim==='function','typed target, owner check and fresh native claim');
  const f=linked?.functions?.find(f=>!f.imported&&f.exports?.includes(exportName));
  demand(f&&JSON.stringify(f.parameters)==='["i32"]'&&JSON.stringify(f.results)==='[]','target must be current linked void1 export');
  const expectedSlot=table.length;
  check();claim(address,expectedSlot);const call=value=>{demand(!failed,'terminal callback registry');try{check();demand(Number.isInteger(value)&&value>=-2147483648&&value<=4294967295,'exact i32 callback argument');const result=invoke(value);demand(result===undefined,'typed void1 callback result');return;}catch(e){failed=true;throw e;}};
  const instance=new WebAssembly.Instance(module,{env:{memory,ff9_callback_void1:call}}),wrapper=instance.exports.ff9_callback_u20;
  demand(typeof wrapper==='function'&&wrapper.length===20,'actual compiled universal20 function');
  // Only grow; never borrow or replace another owner's slot.
  let slot;
  try{
   demand(table.length===expectedSlot,'table changed during owner claim');
   slot=table.grow(1);table.set(slot,wrapper);registerAliases(address,slot,slot);
   demand((resolve(address)>>>0)===slot&&(resolve(slot)>>>0)===slot&&table.get(slot)===wrapper,'registered native/slot aliases');
  }catch(e){failed=true;throw e;}
  const record=Object.freeze({address,slot,exportName,parameters:Object.freeze(['i32']),results:Object.freeze([]),tableArity:20});
  const receipt=Object.freeze({memory,table,binding:record,wrapperSha256:sha});
  records.set(address,{binding:record,wrapper,receipt});return record;
 };
 const registry=Object.freeze({bind,snapshot:()=>Object.freeze({failed,bindings:Object.freeze([...records.values()].map(v=>v.binding)),wrapperSha256:sha})});
 registries.set(registry,{records,table,failed:()=>failed});return registry;
}
