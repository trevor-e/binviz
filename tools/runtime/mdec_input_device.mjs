// Continue a live, initialized table/DMA device at the original movie command
// boundary. Input/output DMA is synchronous and colored output is reordered.
// CPU FIFO packing and exact hardware timing require separate implementations.
const DATA=0x1f801820,CONTROL=0x1f801824,DPCR=0x1f8010f0;
const require=(c,s)=>{if(!c)throw Error('MDEC input device: '+s)},u=v=>v>>>0;
export function createMdecInputDevice({previous,ram,mem,platform,decoder,allowOutputDma=false,onTrace=()=>{}}={}) {
 require(previous?.snapshot&&previous?.history&&ram?.buffer===mem?.buffer&&platform?.read32&&platform?.dispatchDma&&decoder?.decode,'live device, shared RAM, platform and decoder');
 const seed=previous.snapshot();
 require(seed.remaining===0&&[seed.quantY,seed.quantUV,seed.scale].every(t=>t?.length===64)&&seed.dma?.length===2,'completed genuine table upload');
 require(typeof allowOutputDma==='boolean'&&(!allowOutputDma||typeof mem.setUint32==='function'),'output DMA RAM writer');
 let control=seed.control,command=seed.command,remaining=0,statusLow=seed.status&65535,parameters=[],decoded=null,outputOffset=0;
 const dma=structuredClone(seed.dma),history=previous.history(),uploads=structuredClone(seed.uploads),decodes=[],outputDmas=[];
 const quantY=[...seed.quantY],quantUV=[...seed.quantUV],scale=[...seed.scale];
 const record=(operation,address,value)=>{const row={operation,address:u(address),value:u(value)};history.push(row);onTrace('mdec-mmio',row)};
 const register=address=>{
  const a=u(address)&0x1fffffff;require((a&3)===0,'aligned32 access');
  if([DATA,CONTROL,DPCR].includes(a))return {a};
  if(a>=0x1f801080&&a<=0x1f801098){const channel=(a-0x1f801080)>>>4,offset=(a-0x1f801080)&15;require(channel<2&&[0,4,8].includes(offset),'DMA0/1 register');return {a,channel,key:['madr','bcr','chcr'][offset>>>2]};}
  throw Error('MDEC input device: unreviewed port 0x'+a.toString(16));
 };
 // Keep a frame command active while its queued macroblocks await delivery.
 // This aggregate functional model does not model the final FIFO's cycle edge.
 const status=()=>u((decoded?0:0x80040000)|((command>>>2)&0x07800000)|((remaining||decoded)?0x20000000:0)|((control&0x40000000)?0x10000000:0)|((decoded&&(control&0x20000000))?0x08000000:0)|statusLow);
 const byteWords=values=>{const bytes=new Uint8Array(values.length*4),v=new DataView(bytes.buffer);values.forEach((n,i)=>v.setUint32(i*4,n,true));return bytes};
 const validateCommand=value=>{
  require(remaining===0&&!decoded,'ready for command without unread output');
  require(value>>>29===1&&(value&0x01ff0000)===0&&((value>>>27)&3)>=2&&(value&65535)>0,'colored command1 contract');
 };
 const prepare=values=>{
  require(values.length>0&&values.length<=remaining,'bounded remaining parameters');
  if(values.length<remaining)return null;
  // Decode before any device/history/register/guest-memory side effect.
  return decoder.decode({input:byteWords([...parameters,...values]),command,quantY,quantUV,scale});
 };
 const accept=(values,prepared)=>{
  parameters.push(...values);remaining-=values.length;statusLow=(remaining-1)&65535;
  if(!remaining){require(prepared?.bytes instanceof Uint8Array,'prepared complete output');decoded=prepared;outputOffset=0;decodes.push({command,words:parameters.length,consumedHalfwords:prepared.consumedHalfwords,macroblocks:prepared.macroblocks,trailingPaddingHalfwords:prepared.trailingPaddingHalfwords,outputBytes:prepared.outputBytes,mathMode:prepared.mathMode,decoderIdentity:decoder.identity});parameters=[];}
 };
 const read32=address=>{
  const r=register(address);let value;
  if(r.a===CONTROL)value=status();else if(r.a===DPCR)value=platform.read32(r.a);
  else if(r.a===DATA)throw Error('MDEC input device: output FIFO ordering not implemented');
  else value=dma[r.channel][r.key];
  record('read',r.a,value);return value|0;
 };
 const write32=(address,value)=>{
  const r=register(address);value=u(value);let values,prepared;
  if(r.a===DATA){if(remaining){values=[value];prepared=prepare(values);}else validateCommand(value);}
  if(r.channel===1&&r.key==='chcr'&&(value&0x01000000)) {
   require(allowOutputDma,'output DMA not implemented');
   const d=dma[1],words=(d.bcr&65535)*(d.bcr>>>16),offset=d.madr&0x1fffff,blockBytes=((command>>>27)&3)===3?512:768;
   require((control&0x20000000)!==0&&(platform.read32(DPCR)&0x80)!==0&&value===0x01000200,'incrementing request DMA1 enabled');
   require(!remaining&&decoded&&words>0&&words*4<=decoded.outputBytes-outputOffset&&(offset&3)===0&&offset+words*4<=0x200000,'bounded native output DMA');
   require(outputOffset%blockBytes===0&&words*4%blockBytes===0,'whole reordered colored macroblocks');
  }
  if(r.channel===0&&r.key==='chcr'&&(value&0x01000000)) {
   const d=dma[0],words=(d.bcr&65535)*(d.bcr>>>16),offset=d.madr&0x1fffff;
   require((control&0x40000000)!==0&&(platform.read32(DPCR)&8)!==0&&value===0x01000201,'incrementing request DMA0 enabled');
   require(words>0&&words<=remaining&&words<=65535&&(offset&3)===0&&offset+words*4<=0x200000,'bounded native input DMA');
   values=Array.from({length:words},(_,i)=>mem.getUint32(0x80000000+offset+i*4,true));prepared=prepare(values);
  }
  record('write',r.a,value);
  if(r.a===CONTROL){
   if(value&0x80000000){command=0;remaining=0;parameters=[];decoded=null;outputOffset=0;statusLow=0;}
   control=value&0x60000000;
  }else if(r.a===DATA){
   if(remaining)accept(values,prepared);else{command=value;remaining=value&65535;parameters=[];statusLow=remaining-1;}
  }else if(r.a===DPCR)platform.write32(r.a,value);
  else {
   const d=dma[r.channel];d[r.key]=value;
   if(r.channel===0&&r.key==='chcr'&&(value&0x01000000)) {
    const offset=d.madr&0x1fffff,words=values.length;accept(values,prepared);
    d.madr=(offset+words*4)&0xffffff;d.bcr&=65535;d.chcr&=~0x01000000;
    platform.dispatchDma(0);onTrace('mdec-input-dma-complete',{words,address:0x80000000+offset});
   }else if(r.channel===1&&r.key==='chcr'&&(value&0x01000000)) {
    const offset=d.madr&0x1fffff,words=(d.bcr&65535)*(d.bcr>>>16),view=new DataView(decoded.bytes.buffer,decoded.bytes.byteOffset,decoded.bytes.byteLength),start=outputOffset;
    // The core's colored output is already reordered as row-major16x16
    // macroblocks. DMA1 writes that logical layout; CPU DATA reads require
    // the distinct8x8 FIFO packing and remain unsupported.
    for(let i=0;i<words;i++)mem.setUint32(0x80000000+offset+i*4,view.getUint32(start+i*4,true),true);
    outputOffset+=words*4;d.madr=(offset+words*4)&0xffffff;d.bcr&=65535;d.chcr&=~0x01000000;
    const receipt={command,address:0x80000000+offset,words,bytes:words*4,sourceByteOffset:start,pendingOutputBytes:decoded.outputBytes-outputOffset};outputDmas.push(receipt);
    if(outputOffset===decoded.outputBytes){decoded=null;outputOffset=0;}
    onTrace('mdec-output-dma-complete',receipt);platform.dispatchDma(1);
   }
  }
 };
 return Object.freeze({read32,write32,history:()=>structuredClone(history),decodedBytes:()=>decoded?.bytes.slice()??null,snapshot:()=>({control,command,remaining,status:status(),dma:structuredClone(dma),quantY:[...quantY],quantUV:[...quantUV],scale:[...scale],uploads:structuredClone(uploads),decodes:structuredClone(decodes),pendingOutputBytes:decoded?decoded.outputBytes-outputOffset:0,...(allowOutputDma?{outputDmas:structuredClone(outputDmas),outputByteOffset:outputOffset}:{}),scope:allowOutputDma?'Synchronous functional input/output DMA of whole reordered colored macroblocks, genuine platform completion dispatch; approximate decoder math, no FIFO/cycle equivalence proof.':'Synchronous functional command1/input DMA and documented approximate colored decoder. Output FIFO/DMA/callback and cycle equivalence unproven.'})});
}
