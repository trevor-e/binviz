// Reuse current typed resident SDK exports. A declared void import may discard
// an int result only for an explicitly reviewed caller; no ABI padding.
export function createFf9ContinuousSdk({exports:x,linked,assertOwner,discardResults=[],onCall=()=>{}}={}) {
 const demand=(c,s)=>{if(!c)throw Error('continuous SDK: '+s);};
 demand(x&&linked?.functions&&typeof assertOwner==='function','authenticated resident exports and linked facts');
 const allowed=new Set(discardResults),facts=new Map();
 for(const f of linked.functions)if(!f.imported)for(const name of f.exports)facts.set(name,f);
 const i32=v=>Number.isInteger(v)&&v>=-2147483648&&v<=4294967295;
 const bind=(name,parameters,results)=>{
  const f=facts.get(name),fn=x[name];if(!f||typeof fn!=='function')return null;
  const same=JSON.stringify(f.parameters)===JSON.stringify(parameters),discard=results.length===0&&JSON.stringify(f.results)==='["i32"]'&&allowed.has(name);
  if(!same||(!discard&&JSON.stringify(f.results)!==JSON.stringify(results)))return null;
  demand(parameters.every(t=>t==='i32')&&results.every(t=>t==='i32')&&fn.length===parameters.length,'exact typed SDK '+name);
  const invoke=(...args)=>{demand(args.length===parameters.length&&args.every(i32),'exact SDK i32 ABI '+name);assertOwner();const value=fn(...args);demand(f.results.length?i32(value):value===undefined,'actual SDK result '+name);onCall({name,args:args.map(v=>v>>>0),...(f.results.length?{nativeResult:value>>>0}:{}),resultDiscarded:discard});return results.length?value:undefined;};
  Object.defineProperty(invoke,'length',{value:parameters.length});return invoke;
 };
 return Object.freeze({bind});
}
