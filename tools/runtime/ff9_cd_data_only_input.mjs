// CD input for the frozen FF9 cold data-only command hosts. This does not
// decode XA/CDDA or approximate an unknown drive/decoder as silent.
export function createFf9CdDataOnlyInput({getState,assertContext,observe=()=>{}}={}){
 const demand=(c,s)=>{if(!c)throw Error('FF9 CD data-only input: '+s);};
 demand(typeof getState==='function'&&typeof assertContext==='function','owned state and closed command/source authority');
 let calls=0,samples=0,lastState=null;
 const read=(...args)=>{
  demand(args.length===1&&Number.isSafeInteger(args[0])&&args[0]>0&&args[0]<=4096,'bounded exact sample ABI');
  assertContext();const state=getState(),d=state?.device,a=state?.adapter;
  demand(d?.discPresent===true&&a?.discPresent===true&&a.mount?.disc===1,'mounted original drive and archive');
  demand([0,0x20,0xa0].includes(d.mode)&&[0,0x20,0xa0].includes(a.mode),'XA/CDDA or unknown mode requires decoded PCM');
  demand(d.status===2&&d.initializing===false&&typeof d.muted==='boolean','unknown drive playback/initialization state');
  demand(Number.isSafeInteger(d.commands)&&d.commands>=3&&Number.isSafeInteger(a.startupCommands)&&a.startupCommands===d.commands,'coherent original startup commands');
  demand(Array.isArray(d.queue)&&d.queue.length===0&&Array.isArray(d.parameters)&&d.parameters.length===0&&d.irq===0,'pending startup command can change audio authority');
  demand(typeof a.reading==='boolean'&&typeof a.active==='boolean'&&Number.isSafeInteger(a.pendingCount)&&a.pendingCount>=0,'explicit data transport state');
  demand(!a.active||a.reading,'active transport must have original data read');
  demand(a.nextLba===null||(Number.isSafeInteger(a.nextLba)&&a.nextLba>=0),'explicit original data position');
  demand(!a.reading||(a.mode===0xa0&&a.nextLba!==null),'reading needs admitted data-only mode/position');
  // All admitted command/port paths exclude Play, XA enable and Sound Map.
  // Their reset/Init boundary establishes an empty decoder history. This is
  // an explicit zero input for that closed scope, checked again each quantum.
  const pcm=new Int16Array(args[0]*2);calls++;samples+=args[0];lastState=structuredClone(state);
  observe({calls,samples,state:lastState,reason:'closed data-only CD commands; no audio decoder history'});return pcm;
 };
 return Object.freeze({read,snapshot:()=>({calls,samples,lastState:structuredClone(lastState),xaDecoded:false,cddaDecoded:false,hardwareTimingVerified:false})});
}
