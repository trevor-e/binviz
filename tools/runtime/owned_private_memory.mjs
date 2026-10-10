// Identity-addressed private-stack view for the existing bounded BIOS byte host.
// The caller supplies a live verified module owner. No address relocation or
// fabricated module exports: every access rechecks that owner's allocation and
// current stack, and the caller's active execution phase.
export function createOwnedPrivateMemoryView({ram,owner,allocation,active}={}) {
 const demand=(ok,message)=>{if(!ok)throw Error('owned private memory: '+message)};
 demand(ram&&ram.buffer&&Number.isSafeInteger(ram.length)&&typeof ram.subarray==='function'&&typeof ram.set==='function','identity-addressed RAM required');
 demand(Object.isFrozen(owner)&&typeof owner.check==='function'&&typeof owner.snapshot==='function'&&typeof active==='function','live immutable module owner and phase required');
 const {base,end,stackBase,moduleSha256}=allocation??{},buffer=ram.buffer;
 demand([base,end,stackBase].every(Number.isSafeInteger)&&base>=0x80200000&&base<stackBase&&stackBase<=end&&end<=ram.length&&!(base&15)&&!(stackBase&15)&&!(end&15)&&typeof moduleSha256==='string'&&/^[0-9a-f]{64}$/.test(moduleSha256),'explicit private allocation required');
 const check=()=>{
  demand(ram.buffer===buffer,'RAM buffer drift');owner.check();demand(active()===true,'inactive execution phase');const s=owner.snapshot();
  demand(s.moduleSha256===moduleSha256&&s.privateMemoryBase===base&&s.privateMemoryEnd===end&&s.privateStackBase===stackBase,'module allocation drift');
  demand(Number.isSafeInteger(s.privateStackPointer)&&s.privateStackPointer>=base&&s.privateStackPointer<=stackBase&&!(s.privateStackPointer&15),'private stack drift');return s.privateStackPointer;
 };
 const covers=address=>{const low=check();return Number.isSafeInteger(address)&&address>=low&&address<stackBase};
 const range=(address,count)=>{const low=check();demand(Number.isSafeInteger(address)&&Number.isSafeInteger(count)&&count>0&&address>=low&&address+count<=stackBase,'range outside active owned stack');};
 check();return Object.freeze({
  covers,normalize:address=>{demand(covers(address),'invalid private address');return address},
  readBytes:(address,count)=>{range(address,count);return ram.subarray(address,address+count).slice()},
  writeBytes:(address,bytes)=>{demand(bytes instanceof Uint8Array,'byte array required');range(address,bytes.length);ram.set(bytes,address)},
  identity:Object.freeze({base,end,stackBase,moduleSha256,scope:'Live owner-checked active compiler stack; identity-addressed bytes, no guest aliases.'})
 });
}
