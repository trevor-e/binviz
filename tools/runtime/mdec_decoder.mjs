// Fixed private WASM decoder service. Host DMA/FIFO/callback state is separate.
const fail=s=>{throw Error('MDEC decoder: '+s)};
const integer=(v,lo,hi)=>Number.isInteger(v)&&v>=lo&&v<=hi;
const require=(c,s)=>{if(!c)fail(s)};
const digest=async bytes=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),x=>x.toString(16).padStart(2,'0')).join('');
export async function createMdecDecoder({moduleBytes,expectedSha256}={}) {
 require(moduleBytes instanceof Uint8Array&&/^[0-9a-f]{64}$/.test(expectedSha256),'pinned module bytes/hash');
 require(await digest(moduleBytes)===expectedSha256,'module identity');
 const module=await WebAssembly.compile(moduleBytes);
 require(WebAssembly.Module.imports(module).length===0,'isolated decoder without imports');
 const instance=await WebAssembly.instantiate(module),e=instance.exports;
 require(e.memory instanceof WebAssembly.Memory&&typeof e.mdec_buffer==='function'&&typeof e.mdec_decode==='function','typed decoder exports');
 require(e.memory.buffer.byteLength===4194304,'fixed private memory');
 const pointers=Array.from({length:6},(_,i)=>e.mdec_buffer(i)>>>0);
 const sizes=[262140,3145728,64,64,128,16];
 for(let i=0;i<6;i++) {
  require(pointers[i]>0&&pointers[i]+sizes[i]<=e.memory.buffer.byteLength,'buffer extent');
  for(let j=0;j<i;j++)require(pointers[i]>=pointers[j]+sizes[j]||pointers[j]>=pointers[i]+sizes[i],'separate buffers');
 }
 const u8=new Uint8Array(e.memory.buffer),v=new DataView(e.memory.buffer);
 return Object.freeze({
  identity:expectedSha256,
  scope:'Bounded functional RGB24/RGB15 macroblocks; documented approximate IDCT/color math; no silicon, DMA/FIFO or callback parity claim',
  decode({input,command,quantY,quantUV,scale,capacity=3145728,maximumBlocks=4096,mathMode='documented'}={}) {
   require(input instanceof Uint8Array&&input.length>0&&input.length<=262140&&(input.length&3)===0,'parameter byte length');
   require(integer(command,0,0xffffffff)&&(command>>>29)===1&&(command&0x01ff0000)===0&&((command>>>27)&3)>=2&&(command&65535)===input.length/4,'command/input extent');
   for(const [name,table]of [['quantY',quantY],['quantUV',quantUV]])require((Array.isArray(table)||table instanceof Uint8Array)&&table.length===64&&Array.from(table).every(x=>integer(x,0,255)),name+' table');
   require((Array.isArray(scale)||scale instanceof Int16Array)&&scale.length===64&&Array.from(scale).every(x=>integer(x,-32768,32767)),'scale table');
   require(integer(capacity,0,3145728)&&integer(maximumBlocks,1,4096),'output bounds');
   require(['documented','upstream-comparison'].includes(mathMode),'math mode');
   // Inputs/tables are copied; the service never owns or mutates guest memory.
   u8.set(input,pointers[0]);u8.set(quantY,pointers[2]);u8.set(quantUV,pointers[3]);
   for(let i=0;i<64;i++)v.setInt16(pointers[4]+2*i,scale[i],true);
   const status=e.mdec_decode(input.length,command,capacity,maximumBlocks,mathMode==='documented'?1:0);
   require(status===0,'bounded decode refused, status '+status);
   const metadata=Array.from({length:4},(_,i)=>v.getUint32(pointers[5]+4*i,true));
   const [consumedHalfwords,macroblocks,trailingPaddingHalfwords,outputBytes]=metadata;
   require(consumedHalfwords===input.length/2&&macroblocks>0&&macroblocks<=maximumBlocks&&outputBytes<=capacity&&outputBytes===macroblocks*(((command>>>27)&3)===3?512:768),'result contract');
   return {bytes:u8.slice(pointers[1],pointers[1]+outputBytes),consumedHalfwords,macroblocks,trailingPaddingHalfwords,outputBytes,mathMode};
  }
 });
}
