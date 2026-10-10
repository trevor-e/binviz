// Bind explicitly selected function providers to a shared Binviz linked report.
// Native ownership, phase checks and proof admission remain with each provider.
export function createTypedImportRegistry(linked,providers){
 const demand=(ok,message)=>{if(!ok)throw Error('typed import registry: '+message);};
 demand(Array.isArray(linked?.importInventory)&&Array.isArray(providers),'linked import inventory and provider array required');
 const key=(module,field)=>JSON.stringify([module,field]),imports=new Map(),bound=new Map();
 for(const i of linked.importInventory){const k=key(i.module,i.field);demand(!imports.has(k),'duplicate linked import identity');imports.set(k,i);}
 const i32=value=>Number.isInteger(value)&&value>=-2147483648&&value<=4294967295;
 for(const p of providers){
  demand(p&&typeof p.module==='string'&&typeof p.field==='string'&&typeof p.invoke==='function','explicit module/field/function provider required');
  const k=key(p.module,p.field),i=imports.get(k);demand(i?.kind==='function','selected provider must be a current function import');demand(!bound.has(k),'duplicate selected provider');
  demand(Array.isArray(p.parameters)&&Array.isArray(p.results)&&JSON.stringify(p.parameters)===JSON.stringify(i.parameters)&&JSON.stringify(p.results)===JSON.stringify(i.results),'provider signature differs from linked import');
  demand(p.parameters.every(t=>t==='i32')&&p.results.length<=1&&p.results.every(t=>t==='i32'),'only reviewed i32/void signatures supported');
  const parameters=Object.freeze([...p.parameters]),results=Object.freeze([...p.results]),invoke=p.invoke;
  const call=(...args)=>{demand(args.length===parameters.length,'exact argument count');demand(args.every(i32),'exact i32 input domain');const result=invoke(...args);demand(results.length?i32(result):result===undefined,'provider result differs from linked signature');return result;};
  Object.defineProperty(call,'length',{value:parameters.length});bound.set(k,Object.freeze(call));
 }
 return Object.freeze({get:(module,field)=>bound.get(key(module,field))??null,size:bound.size,moduleSha256:typeof linked.sha256==='string'?linked.sha256:null});
}
