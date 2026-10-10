// Functional retail v2 GPU subset. Unknown commands fail at their boundary.
// References and scope are documented in docs/gpu-host.md and docs/gpu-raster.md.
export function createGpuDevice({onTrace=()=>{},onIrq=()=>{},spriteSampling='strict'}={}) {
  if(!['strict','ordinary'].includes(spriteSampling))throw new Error('invalid GPU sprite sampling mode');
  const vram=new Uint16Array(1024*512), attrs=new Uint32Array(7);
  let status=0x14802000,latch=0,pending=[],transfer=null,displayStart=0,displayX=0xc00200,displayY=0x40010,displayMode=0;
  // Functional beam phase supplied by the frame host, independent of GP1 reset.
  // Timing/cycles remain the caller's responsibility.
  let scanout={vblank:true,field:1,line:0};
  const refreshScanout=()=>{
    const interlace=!!(status&0x400000),high=interlace&&!!(status&0x80000);
    const odd=scanout.vblank?0:(high?scanout.field:(scanout.line&1));
    status=((status&~0x80002000)|((interlace?scanout.field:1)<<13)|(odd?0x80000000:0))>>>0;
  };
  const setScanoutPhase=({vblank,field,line})=>{
    if(typeof vblank!=='boolean'||!Number.isInteger(field)||field<0||field>1||!Number.isInteger(line)||line<0||line>1023)
      throw new Error('invalid GPU scanout phase');
    scanout={vblank,field,line};refreshScanout();
  };
  const emit=(kind,detail)=>onTrace(kind,detail);
  const reset=()=>{status=0x14802000;attrs.fill(0);pending=[];transfer=null;displayStart=0;displayX=0xc00200;displayY=0x40010;displayMode=0;};
  const refresh=()=>{
    const mode=(status>>>29)&3;
    status=(status&~0x02000000)|((mode===1||mode===2&&(status&0x10000000)||mode===3&&(status&0x08000000))?0x02000000:0);
  };
  const transferIndex=t=>(((t.y+Math.floor(t.done/t.width))&511)*1024+((t.x+t.done%t.width)&1023));
  const finishTransfer=()=>{emit('gpu-transfer-complete',{...transfer});transfer=null;status=(status|0x14000000)&~0x08000000;refresh();};
  const writePixel=pixel=>{
    const t=transfer,index=transferIndex(t);
    if(!(attrs[6]&2)||!(vram[index]&0x8000))vram[index]=pixel|((attrs[6]&1)?0x8000:0);
    t.done++;
    if(t.done===t.width*t.height)finishTransfer();
  };
  // VRAM copy coordinates are absolute and wrap independently in X/Y.
  // Distinct overlapping rectangles require hardware read/write-order proof.
  const cyclicParts=(start,length,limit)=>start+length<=limit?[[start,start+length]]:[[start,limit],[0,start+length-limit]];
  const axisIntersects=(a,b,length,limit)=>cyclicParts(a,length,limit).some(([lo,hi])=>
    cyclicParts(b,length,limit).some(([otherLo,otherHi])=>lo<otherHi&&otherLo<hi));
  const copyVram=words=>{
    const sx=words[1]&1023,sy=(words[1]>>>16)&511,x=words[2]&1023,y=(words[2]>>>16)&511,
      width=(((words[3]&65535)-1)&1023)+1,height=(((words[3]>>>16)-1)&511)+1;
    if((sx!==x||sy!==y)&&axisIntersects(sx,x,width,1024)&&axisIntersects(sy,y,height,512))
      throw new Error('unsupported GPU distinct overlapping VRAM copy');
    for(let row=0;row<height;row++)for(let col=0;col<width;col++){
      const source=((sy+row)&511)*1024+((sx+col)&1023),destination=((y+row)&511)*1024+((x+col)&1023);
      if(!(attrs[6]&2)||!(vram[destination]&0x8000))vram[destination]=vram[source]|((attrs[6]&1)?0x8000:0);
    }
    emit('gpu-copy',{sourceX:sx,sourceY:sy,x,y,width,height});
  };
  // Variable rectangles (GP0 60..63), modulated sprites and opaque grid lines.
  // The untextured ABE path reuses the existing RGB555 blend rules, clipping
  // and mask handling; bit24 texture-color mode is ignored for solid rectangles.
  const signed11=value=>(value&1024)?(value&2047)-2048:value&2047;
  const vertex=word=>({x:signed11(word),y:signed11(word>>>16)});
  const offset=()=>({x:signed11(attrs[5]),y:signed11(attrs[5]>>>11)});
  const drawPixel=(x,y,pixel)=>{
    if(x<0||x>=1024||y<0||y>=512||x<(attrs[3]&1023)||y<(attrs[3]>>>10)||
       x>(attrs[4]&1023)||y>(attrs[4]>>>10))return;
    const index=y*1024+x;
    if(!(attrs[6]&2)||!(vram[index]&0x8000))vram[index]=pixel|((attrs[6]&1)?0x8000:0);
  };
  const color15=(rgb,dither=0)=>{
    let result=0;
    for(let c=0;c<3;c++)result|=Math.max(0,Math.min(31,((rgb>>>(c*8)&255)+dither)>>3))<<(c*5);
    return result;
  };
  const raster=words=>{
    const command=words[0]>>>24,a=vertex(words[1]),o=offset();
    // Active display exclusion/overflow wrap require a separate hardware proof.
    if((status&0x480000)===0x480000)throw new Error('unsupported GPU interlaced rasterization');
    const x=a.x+o.x,y=a.y+o.y;
    if(x<-1024||x>1023||y<-1024||y>1023)throw new Error('unsupported GPU drawing offset overflow');
    if(!(status&0x800000)&&!(attrs[1]&0x400)) {
      // A solid rectangle wholly outside the active display footprint needs no
      // display-exclusion behavior. Keep overlap and other primitives
      // explicit until their exclusion rules are reviewed. This admits normal
      // double-buffered drawing without pretending that DFE was enabled.
      const solid=command>=0x60&&command<=0x63,wh=words[2],w=wh&1023,h=(wh>>>16)&511;
      if(!solid||((displayMode&0x10)&&(displayMode&0x20)))throw new Error('unsupported GPU drawing with display exclusion');
      const divisor=displayMode&0x40?7:[10,8,5,4][displayMode&3],
        sx=displayStart&1023,sy=(displayStart>>>10)&511,
        logicalWidth=Math.max(0,Math.ceil(((displayX>>>12)-(displayX&4095))/divisor)),
        // Display coordinates are VRAM halfwords; RGB24 uses three bytes/pixel.
        // Round outward to conservatively include a partially selected halfword.
        sw=Math.min(1024,Math.ceil(logicalWidth*((displayMode&0x10)?1.5:1))),
        sh=Math.min(512,Math.max(0,(displayY>>>10)-(displayY&1023))),
        left=Math.max(0,x,attrs[3]&1023),right=Math.min(1023,x+w-1,attrs[4]&1023),
        top=Math.max(0,y,attrs[3]>>>10),bottom=Math.min(511,y+h-1,attrs[4]>>>10);
      const intersects=right>=left&&bottom>=top&&cyclicParts(sx,sw,1024).some(([lo,hi])=>left<hi&&lo<=right)&&
        cyclicParts(sy,sh,512).some(([lo,hi])=>top<hi&&lo<=bottom);
      if(intersects)throw new Error('unsupported GPU drawing with display exclusion');
    }
    if(command===0x40) {
      const b=vertex(words[2]),dx=b.x-a.x,dy=b.y-a.y;
      if(Math.abs(dx)>1023||Math.abs(dy)>511){emit('gpu-line',{dropped:true});return;}
      if(dx&&dy&&Math.abs(dx)!==Math.abs(dy))throw new Error('unsupported GPU line slope');
      if(b.x+o.x<-1024||b.x+o.x>1023||b.y+o.y<-1024||b.y+o.y>1023)
        throw new Error('unsupported GPU drawing offset overflow');
      const dither=[-4,0,-3,1,2,-2,3,-1,-3,1,-4,0,3,-1,2,-2];
      const length=Math.max(Math.abs(dx),Math.abs(dy));
      for(let i=0;i<=length;i++){
        const px=x+Math.sign(dx)*i,py=y+Math.sign(dy)*i;
        drawPixel(px,py,color15(words[0],attrs[1]&0x200?dither[((py&3)<<2)|(px&3)]:0));
      }
      emit('gpu-line',{x,y,endX:b.x+o.x,endY:b.y+o.y});return;
    }
    const textured=command===0x64||command===0x66,semi=command===0x62||command===0x63||command===0x66,wh=words[textured?3:2],width=wh&1023,height=(wh>>>16)&511;
    if(!width||!height){emit('gpu-rectangle',{x,y,width,height,textured});return;}
    // Short odd widths use the ordinary first span. This is a bounded inference
    // from the documented16-pixel error cadence, not a silicon-measured oracle.
    // Strict mode retains that frontier. Explicit ordinary mode uses conventional
    // wrapped texture coordinates, without the retail 16-sample hardware error.
    if(textured&&spriteSampling==='strict'&&((words[2]&1)||((width&1)&&(width>=16||(attrs[1]&0x1000)))))
      throw new Error('unsupported GPU odd sprite U/width sampling');
    if(textured&&(attrs[1]&0x800))throw new Error('unsupported GPU texture page Y bit11');
    const left=Math.max(0,x,attrs[3]&1023),right=Math.min(1023,x+width-1,attrs[4]&1023),
      top=Math.max(0,y,attrs[3]>>>10),bottom=Math.min(511,y+height-1,attrs[4]>>>10);
    const rgb=words[0],plain=color15(rgb),uv=words[2],page=attrs[1],window=attrs[2],depth=(page>>>7)&3,
      baseX=(page&15)*64,baseY=(page&16)?256:0,clutX=((uv>>>16)&63)*16,clutY=(uv>>>22)&511,
      maskX=(window&31)*8,maskY=((window>>>5)&31)*8,
      setX=(((window>>>10)&31)&(window&31))*8,setY=(((window>>>15)&31)&((window>>>5)&31))*8;
    for(let py=top;py<=bottom;py++)for(let px=left;px<=right;px++){
      let pixel=plain;
      if(textured){
        const u=((((uv&255)+(px-x)*(page&0x1000?-1:1))&255)&~maskX)|setX,
          v=(((((uv>>>8)&255)+(py-y)*(page&0x2000?-1:1))&255)&~maskY)|setY;
        const divisor=depth===0?4:depth===1?2:1,
          texel=vram[((baseY+v)&511)*1024+((baseX+Math.floor(u/divisor))&1023)];
        pixel=depth<2?vram[clutY*1024+((clutX+((texel>>>((u%divisor)*(depth===0?4:8)))&(depth===0?15:255)))&1023)]:texel;
        if(pixel===0)continue;
        let modulated=pixel&0x8000;
        for(let c=0;c<3;c++)modulated|=Math.min(31,((pixel>>>(c*5)&31)*(rgb>>>(c*8)&255))>>>7)<<(c*5);
        pixel=modulated;
        // Textured ABE applies only to fetched STP texels; zero was discarded
        // before modulation, so semi-transparent black (8000) still blends.
      }
        // Solid ABE does not need an STP bit. Textured ABE keeps its fetched
        // STP condition and transparent-zero behavior. E6 still owns write masks.
        if(semi&&(!textured||(pixel&0x8000))) {
          const background=vram[py*1024+px],mode=(page>>>5)&3;
          let blended=pixel&0x8000;
          for(let c=0;c<3;c++) {
            const back=(background>>>(c*5))&31,front=(pixel>>>(c*5))&31;
            const channel=mode===0?(back+front)>>>1:mode===1?Math.min(31,back+front):
              mode===2?Math.max(0,back-front):Math.min(31,back+(front>>>2));
            blended|=channel<<(c*5);
          }
          pixel=blended;
        }
      drawPixel(px,py,pixel);
    }
    emit('gpu-rectangle',{x,y,width,height,textured});
  };
  const gp0=word=>{
    word>>>=0;
    if(transfer) {
      if(transfer.direction!=='upload')throw new Error('GPU data read transfer is active');
      writePixel(word&65535);if(transfer)writePixel(word>>>16);return;
    }
    if(pending.length) {
      pending.push(word);
      if(pending.length===(((pending[0]>>>24)===0x64||(pending[0]>>>24)===0x66)||(pending[0]>>>29)===4?4:3)) {
        const command=pending[0]>>>24;
        if((pending[0]>>>29)===4) {
          const words=pending;pending=[];copyVram(words);status|=0x04000000;refresh();return;
        }
        if((command>=0x60&&command<=0x63)||command===0x64||command===0x66||command===0x40) {
          const words=pending;pending=[];status|=0x04000000;refresh();raster(words);return;
        }
        const [rgb,xy,wh]=pending;
        if((rgb>>>29)===5||(rgb>>>29)===6) {
          transfer={direction:(rgb>>>29)===5?'upload':'download',x:xy&1023,y:(xy>>>16)&511,
            width:(((wh&65535)-1)&1023)+1,height:(((wh>>>16)-1)&511)+1,done:0};
          pending=[];status&=~0x04000000;
          if(transfer.direction==='download')status|=0x08000000;
          emit('gpu-transfer-start',{...transfer});refresh();return;
        }
        const x=xy&0x3f0,y=(xy>>>16)&511,w=((wh&1023)+15)&~15,h=(wh>>>16)&511;
        const color=((rgb>>>3)&31)|(((rgb>>>11)&31)<<5)|(((rgb>>>19)&31)<<10);
        for(let dy=0;dy<h;dy++)for(let dx=0;dx<w;dx++)vram[((y+dy)&511)*1024+((x+dx)&1023)]=color;
        emit('gpu-fill',{x,y,width:w,height:h,color});
        pending=[];status|=0x04000000;refresh();
      }
      return;
    }
    const command=word>>>24;
    if(command===0||command===1) {emit('gpu-nop',{command});return;}
    if(command===2||(command>=0x60&&command<=0x63)||command===0x64||command===0x66||command===0x40||(word>>>29)===4||(word>>>29)===5||(word>>>29)===6) {pending=[word];status&=~0x04000000;refresh();return;}
    if(command===0x1f) {status|=0x01000000;onIrq();return;}
    if(command>=0xe1&&command<=0xe6) {
      const i=command-0xe0,masks=[0,0x3fff,0xfffff,0xfffff,0xfffff,0x3fffff,3];
      attrs[i]=word&masks[i];
      if(i===1)status=((status&~0x87ff)|(word&0x7ff)|((word&0x800)<<4))>>>0;
      if(i===6)status=((status&~0x1800)|((word&3)<<11))>>>0;
      emit('gpu-attribute',{command,value:attrs[i]});return;
    }
    throw new Error(`unsupported GPU GP0 command 0x${command.toString(16)}`);
  };
  const gp1=word=>{
    word>>>=0;const command=(word>>>24)&0x3f,param=word&0xffffff;
    emit('gpu-control',{command,param});
    switch(command) {
      case 0:reset();break;
      case 1:pending=[];transfer=null;status=(status|0x14000000)&~0x08000000;break;
      case 2:status&=~0x01000000;break;
      case 3:status=(status&~0x800000)|((param&1)<<23);break;
      case 4:status=((status&~0x62000000)|((param&3)<<29)|((param&3)===1||(param&3)===2?0x02000000:0))>>>0;break;
      case 5:displayStart=param&0x7ffff;break;
      case 6:displayX=param;break;
      case 7:displayY=param&0xfffff;break;
      case 8:displayMode=param&0x7f;status=((status&~0x7f0000)|((param&0x3f)<<17)|((param&0x40)<<10))>>>0;break;
      case 9:if(param&1)throw new Error('GPU 2-MiB VRAM mode is not implemented');break;
      default:
        if(command>=0x10&&command<=0x1f) {
          const index=param&15;
          if(index>=2&&index<=5)latch=attrs[index];
          else if(index===7)latch=2;
          else if(index===8)latch=0;
        }else throw new Error(`unsupported GPU GP1 command 0x${command.toString(16)}`);
    }
    refresh();refreshScanout();
  };
  const read32=address=>{
    const physical=(address>>>0)&0x1fffffff;
    if(physical===0x1f801810) {
      if(transfer?.direction==='download') {
        const t=transfer,lo=vram[transferIndex(t)];t.done++;
        // Hardware supplies one extra halfword for an odd rectangle. Its value
        // is unspecified by the reference; this functional device uses zero.
        const hi=t.done<t.width*t.height?vram[transferIndex(t)]:0;if(t.done<t.width*t.height)t.done++;
        latch=(lo|(hi<<16))>>>0;if(t.done===t.width*t.height)finishTransfer();
      }
      return latch>>>0;
    }
    if(physical===0x1f801814)return status>>>0;
    throw new Error(`unsupported GPU read32 0x${(address>>>0).toString(16)}`);
  };
  const write32=(address,value)=>{
    const physical=(address>>>0)&0x1fffffff;
    if(physical===0x1f801810)return gp0(value);
    if(physical===0x1f801814)return gp1(value);
    throw new Error(`unsupported GPU write32 0x${(address>>>0).toString(16)}`);
  };
  return {vram,gp0,gp1,read32,write32,setScanoutPhase,snapshot:()=>({status:status>>>0,latch:latch>>>0,
    pendingWords:pending.length,transfer:transfer?{...transfer}:null,displayStart,displayX,displayY,displayMode,scanout:{...scanout},attributes:Array.from(attrs)})};
}
