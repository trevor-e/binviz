// Services for the original movie DMA1 callback. Game logic remains in the
// authenticated WASM export; live guest buffers and stack select each transfer.
export function createFf9MovieOutputServices({ram,mem,owner,platform,getMdec,getBios,resident,passiveVsync,assertContext,assertSdk}={}) {
 const demand=(ok,s)=>{if(!ok)throw Error('movie output services: '+s);};
 demand(ram?.buffer===mem?.buffer&&owner?.check&&owner?.snapshot&&owner?.getExports&&platform?.snapshot&&typeof getMdec==='function'&&typeof getBios==='function'&&typeof passiveVsync==='function'&&passiveVsync.length===1&&typeof assertContext==='function'&&typeof assertSdk==='function','initialized authenticated services');
 const buffer=ram.buffer,counts={callbacks:0,memsets:0,uploads:0,reads:0,writes:0,queries:0};let active=false,failed=false,entryStack=0,lastUploadResult=null;
 const i32=v=>Number.isInteger(v)&&v>=-2147483648&&v<=4294967295;
 const check=()=>{
  demand(!failed,'terminal callback failure');demand(ram.buffer===buffer&&mem.buffer===buffer,'memory buffer drift');owner.check();assertContext();assertSdk();
  const p=platform.snapshot();demand(p.inCallback===1&&p.dmaCallbacks[1]===0x80199554&&mem.getUint32(0x80067904,true)===0x80199554,'original DMA1 callback ownership');
  const s=owner.snapshot();demand(Number.isInteger(s.privateStackPointer)&&!(s.privateStackPointer&15)&&s.privateStackPointer>=s.privateMemoryBase&&s.privateStackPointer<=s.privateStackBase,'owned compiler stack');return s;
 };
 const inside=()=>{const s=check();demand(active,'active original callback');return s;};
 const invokeNative=(...args)=>{
  demand(args.length===0,'exact native callback void0 ABI');const s=check();demand(!active,'nonrecursive native callback');
  const fn=owner.getExports().sub_80199554;demand(typeof fn==='function'&&fn.length===0,'authenticated native callback export');entryStack=s.privateStackPointer;active=true;
  try{const result=fn();demand(result===undefined,'native callback void result');demand(check().privateStackPointer===entryStack,'native callback stack restored');counts.callbacks++;return;}
  catch(e){failed=true;throw e;}finally{active=false;}
 };
 const memset=(...args)=>{
  demand(args.length===3&&args.every(i32),'exact memset void3 i32 ABI');const s=inside();
  demand((args[0]>>>0)===s.privateStackPointer+8&&(args[1]>>>0)===0&&(args[2]>>>0)===8&&s.privateStackPointer+16<=entryStack,'original eight-byte local memset');
  const bios=getBios();demand(typeof bios?.memset==='function','existing BIOS memory service');const result=bios.memset(...args);demand((result>>>0)===(args[0]>>>0),'original BIOS result discarded');counts.memsets++;return;
 };
 const loadImage=(...args)=>{
  demand(args.length===2&&args.every(i32),'exact LoadImage void2 i32 ABI');inside();
  const index=mem.getUint8(0x8019dc90),selected=mem.getUint32(0x8019f4ec,true),rect=args[0]>>>0,data=args[1]>>>0;
  demand(index<4&&selected<=1&&rect===0x8019f488+index*8&&data===mem.getUint32(0x8019f4e8-selected*4,true),'current native rectangle and completed buffer');
  const x=mem.getInt16(rect,true),y=mem.getInt16(rect+2,true),w=mem.getInt16(rect+4,true),h=mem.getInt16(rect+6,true),bytes=w*h*2;
  demand(x>=0&&y>=0&&w>0&&h>0&&x+w<=1024&&y+h<=512,'bounded VRAM rectangle');
  demand(!(data&3)&&data>=0x80000000&&data+bytes<=0x80200000,'bounded native image payload');
  const fn=resident?.ff9_gpu_body_load_image;demand(typeof fn==='function'&&fn.length===2,'existing typed SDK LoadImage');const result=fn(...args);demand(i32(result),'native SDK LoadImage result discarded');lastUploadResult=result;counts.uploads++;return;
 };
 const read32=(...args)=>{demand(args.length===1&&args.every(i32),'exact read32 i32 ABI');inside();const value=getMdec().read32(...args);counts.reads++;return value;};
 const write32=(...args)=>{demand(args.length===2&&args.every(i32),'exact write32 i32 ABI');inside();const value=getMdec().write32(...args);counts.writes++;return value;};
 const vsync=(...args)=>{demand(args.length===1&&args.every(i32)&&(args[0]|0)===-1,'passive VSync int1 query');inside();const result=passiveVsync(...args);demand(i32(result),'native VSync result');counts.queries++;return result;};
 for(const [fn,length]of [[invokeNative,0],[memset,3],[loadImage,2],[read32,1],[write32,2],[vsync,1]])Object.defineProperty(fn,'length',{value:length});
 return Object.freeze({invokeNative,memset,loadImage,read32,write32,vsync,diagnostics:()=>Object.freeze({...counts,active,failed,lastUploadResult,scope:'Original native DMA1 callback on live stack/buffers and existing BIOS/SDK/MDEC services. Functional GPU/decoder; no silicon or cycle claim.'})});
}
